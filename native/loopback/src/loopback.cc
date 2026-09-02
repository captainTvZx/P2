// Windows WASAPI loopback: hoparlörden çıkan sesi yakalar.
// Chromium'un getDisplayMedia yolu cihazın mix biçimini (7.1, 32-bit float vb.)
// açamadığı için sesi burada cihazın kendi biçiminde alıp stereoya indiriyoruz.
#include <node_api.h>

#ifdef _WIN32
#include <windows.h>
#include <mmdeviceapi.h>
#include <audioclient.h>
#include <audioclientactivationparams.h>
#include <functiondiscoverykeys_devpkey.h>
#include <mmreg.h>
#include <atomic>
#include <string>
#include <thread>
#include <vector>

namespace {

struct Capture {
  std::thread worker;
  std::atomic<bool> running{false};
  napi_threadsafe_function emit = nullptr;
  std::string error;
};

Capture* capture = nullptr;

struct Chunk {
  std::vector<float> samples;   // stereo, interleaved
  int sampleRate = 48000;
};

// İşlem bazlı loopback için etkinleştirme geri çağrısı (Windows 10 20348+ / Windows 11).
class ActivationHandler : public IActivateAudioInterfaceCompletionHandler, public IAgileObject {
 public:
  HANDLE done = CreateEvent(nullptr, TRUE, FALSE, nullptr);
  HRESULT result = E_FAIL;
  IAudioClient* client = nullptr;

  ~ActivationHandler() { if (done) CloseHandle(done); }

  STDMETHODIMP ActivateCompleted(IActivateAudioInterfaceAsyncOperation* operation) override {
    IUnknown* unknown = nullptr;
    HRESULT activation = E_FAIL;
    operation->GetActivateResult(&activation, &unknown);
    result = activation;
    if (SUCCEEDED(activation) && unknown) unknown->QueryInterface(__uuidof(IAudioClient), reinterpret_cast<void**>(&client));
    if (unknown) unknown->Release();
    SetEvent(done);
    return S_OK;
  }
  STDMETHODIMP QueryInterface(REFIID id, void** object) override {
    if (id == __uuidof(IUnknown) || id == __uuidof(IActivateAudioInterfaceCompletionHandler)) *object = static_cast<IActivateAudioInterfaceCompletionHandler*>(this);
    else if (id == __uuidof(IAgileObject)) *object = static_cast<IAgileObject*>(this);
    else { *object = nullptr; return E_NOINTERFACE; }
    return S_OK;
  }
  STDMETHODIMP_(ULONG) AddRef() override { return 2; }
  STDMETHODIMP_(ULONG) Release() override { return 1; }
};

// Kendi sesimizi hariç tutan işlem loopback'i: karşı tarafın sesi yayına geri karışmaz.
IAudioClient* ActivateProcessLoopback(WAVEFORMATEX* format, HANDLE* eventHandle, std::string* error) {
  AUDIOCLIENT_ACTIVATION_PARAMS params = {};
  params.ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK;
  params.ProcessLoopbackParams.TargetProcessId = GetCurrentProcessId();
  params.ProcessLoopbackParams.ProcessLoopbackMode = PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE;

  PROPVARIANT activation = {};
  activation.vt = VT_BLOB;
  activation.blob.cbSize = sizeof(params);
  activation.blob.pBlobData = reinterpret_cast<BYTE*>(&params);

  ActivationHandler handler;
  IActivateAudioInterfaceAsyncOperation* operation = nullptr;
  HRESULT hr = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, __uuidof(IAudioClient), &activation, &handler, &operation);
  if (operation) operation->Release();
  if (FAILED(hr)) { *error = "islem loopback etkinlestirilemedi"; return nullptr; }
  if (WaitForSingleObject(handler.done, 3000) != WAIT_OBJECT_0 || FAILED(handler.result) || !handler.client) {
    *error = "islem loopback yanit vermedi";
    return nullptr;
  }

  IAudioClient* client = handler.client;
  const REFERENCE_TIME duration = 2000000;   // 200 ms
  hr = client->Initialize(AUDCLNT_SHAREMODE_SHARED,
                          AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                          duration, 0, format, nullptr);
  if (FAILED(hr)) {
    char message[128];
    snprintf(message, sizeof(message), "islem loopback Initialize hatasi (0x%08lX)", static_cast<unsigned long>(hr));
    *error = message;
    client->Release();
    return nullptr;
  }
  *eventHandle = CreateEvent(nullptr, FALSE, FALSE, nullptr);
  if (FAILED(client->SetEventHandle(*eventHandle))) { client->Release(); *error = "olay tanitilamadi"; return nullptr; }
  return client;
}

// Cihazın kanal düzenini stereoya indirger: ön sol/sağ tam, merkez ve arkalar kısık.
void Downmix(const float* input, UINT32 frames, WORD channels, std::vector<float>& out) {
  out.resize(static_cast<size_t>(frames) * 2);
  for (UINT32 frame = 0; frame < frames; ++frame) {
    const float* source = input + static_cast<size_t>(frame) * channels;
    float left = 0.f, right = 0.f;
    if (channels == 1) {
      left = right = source[0];
    } else {
      left = source[0];
      right = source[1];
      if (channels >= 4) {
        // 5.1/7.1: merkez ve arka kanalları -3 dB ile karıştır.
        float center = channels >= 3 ? source[2] * 0.707f : 0.f;
        float rest = 0.f;
        for (WORD channel = 4; channel < channels; ++channel) rest += source[channel];
        rest *= 0.5f;
        left += center + rest * 0.707f;
        right += center + rest * 0.707f;
      }
    }
    out[static_cast<size_t>(frame) * 2] = left > 1.f ? 1.f : (left < -1.f ? -1.f : left);
    out[static_cast<size_t>(frame) * 2 + 1] = right > 1.f ? 1.f : (right < -1.f ? -1.f : right);
  }
}

void ConvertPcm16(const BYTE* input, UINT32 frames, WORD channels, std::vector<float>& scratch) {
  const int16_t* samples = reinterpret_cast<const int16_t*>(input);
  scratch.resize(static_cast<size_t>(frames) * channels);
  for (size_t index = 0; index < scratch.size(); ++index) scratch[index] = samples[index] / 32768.f;
}

// JS tarafına çağrı: {sampleRate, samples: Float32Array}
void CallJs(napi_env env, napi_value callback, void* /*context*/, void* data) {
  Chunk* chunk = static_cast<Chunk*>(data);
  if (env == nullptr || callback == nullptr) { delete chunk; return; }

  napi_value buffer, array, payload, rate, undefined;
  void* raw = nullptr;
  size_t bytes = chunk->samples.size() * sizeof(float);
  if (napi_create_arraybuffer(env, bytes, &raw, &buffer) == napi_ok) {
    memcpy(raw, chunk->samples.data(), bytes);
    napi_create_typedarray(env, napi_float32_array, chunk->samples.size(), buffer, 0, &array);
    napi_create_object(env, &payload);
    napi_create_int32(env, chunk->sampleRate, &rate);
    napi_set_named_property(env, payload, "sampleRate", rate);
    napi_set_named_property(env, payload, "samples", array);
    napi_get_undefined(env, &undefined);
    napi_call_function(env, undefined, callback, 1, &payload, nullptr);
  }
  delete chunk;
}

void CaptureLoop() {
  HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
  const bool comReady = SUCCEEDED(hr);
  IMMDeviceEnumerator* enumerator = nullptr;
  IMMDevice* device = nullptr;
  IAudioClient* client = nullptr;
  IAudioCaptureClient* reader = nullptr;
  WAVEFORMATEX* mixFormat = nullptr;
  HANDLE ready = nullptr;

  // 1) Tercih: kendi çıkışımızı hariç tutan işlem loopback'i (Windows 11 / 10 20348+).
  //    Böylece aramadaki karşı tarafın sesi yayına geri karışmaz.
  WAVEFORMATEX processFormat = {};
  processFormat.wFormatTag = WAVE_FORMAT_IEEE_FLOAT;
  processFormat.nChannels = 2;
  processFormat.nSamplesPerSec = 48000;
  processFormat.wBitsPerSample = 32;
  processFormat.nBlockAlign = 8;
  processFormat.nAvgBytesPerSec = 48000 * 8;
  processFormat.cbSize = 0;

  std::string activationError;
  client = ActivateProcessLoopback(&processFormat, &ready, &activationError);
  bool processMode = client != nullptr;
  WAVEFORMATEX* format = processMode ? &processFormat : nullptr;

  // 2) Yedek: uç nokta loopback'i (her şeyi yakalar, kendi sesimiz dahil).
  if (!processMode) {
    if (SUCCEEDED(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL,
                                   __uuidof(IMMDeviceEnumerator), reinterpret_cast<void**>(&enumerator))) &&
        SUCCEEDED(enumerator->GetDefaultAudioEndpoint(eRender, eConsole, &device)) &&
        SUCCEEDED(device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, reinterpret_cast<void**>(&client))) &&
        SUCCEEDED(client->GetMixFormat(&mixFormat))) {
      hr = client->Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2000000, 0, mixFormat, nullptr);
      if (SUCCEEDED(hr)) {
        format = mixFormat;
      } else {
        char message[160];
        snprintf(message, sizeof(message), "%s; uc nokta loopback de acilamadi (0x%08lX)",
                 activationError.c_str(), static_cast<unsigned long>(hr));
        if (capture) capture->error = message;
      }
    } else if (capture) {
      capture->error = activationError.empty() ? "Varsayilan ses cihazi acilamadi" : activationError;
    }
  }

  if (format && client && SUCCEEDED(client->GetService(__uuidof(IAudioCaptureClient), reinterpret_cast<void**>(&reader))) &&
      SUCCEEDED(client->Start())) {
    const WORD channels = format->nChannels;
    const int rate = static_cast<int>(format->nSamplesPerSec);
    const bool isFloat = format->wFormatTag == WAVE_FORMAT_IEEE_FLOAT ||
                         (format->wFormatTag == WAVE_FORMAT_EXTENSIBLE &&
                          reinterpret_cast<WAVEFORMATEXTENSIBLE*>(format)->SubFormat == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT);
    std::vector<float> scratch;
    std::vector<float> stereo;

    while (capture && capture->running) {
      if (processMode && ready) WaitForSingleObject(ready, 200);
      UINT32 packet = 0;
      if (FAILED(reader->GetNextPacketSize(&packet))) break;
      if (packet == 0) { if (!processMode) Sleep(8); continue; }
      while (packet > 0 && capture->running) {
        BYTE* data = nullptr;
        UINT32 frames = 0;
        DWORD flags = 0;
        if (FAILED(reader->GetBuffer(&data, &frames, &flags, nullptr, nullptr))) break;
        if (frames > 0) {
          if (flags & AUDCLNT_BUFFERFLAGS_SILENT) {
            stereo.assign(static_cast<size_t>(frames) * 2, 0.f);
          } else if (isFloat) {
            Downmix(reinterpret_cast<const float*>(data), frames, channels, stereo);
          } else if (format->wBitsPerSample == 16) {
            ConvertPcm16(data, frames, channels, scratch);
            Downmix(scratch.data(), frames, channels, stereo);
          } else {
            stereo.assign(static_cast<size_t>(frames) * 2, 0.f);
          }
          if (capture->emit) {
            Chunk* chunk = new Chunk();
            chunk->samples = stereo;
            chunk->sampleRate = rate;
            if (napi_call_threadsafe_function(capture->emit, chunk, napi_tsfn_nonblocking) != napi_ok) delete chunk;
          }
        }
        reader->ReleaseBuffer(frames);
        if (FAILED(reader->GetNextPacketSize(&packet))) break;
      }
    }
    client->Stop();
  } else if (capture && capture->error.empty()) {
    capture->error = "Ses akisi baslatilamadi";
  }

  if (capture && !capture->error.empty()) capture->running = false;
  if (mixFormat) CoTaskMemFree(mixFormat);
  if (ready) CloseHandle(ready);
  if (reader) reader->Release();
  if (client) client->Release();
  if (device) device->Release();
  if (enumerator) enumerator->Release();
  if (comReady) CoUninitialize();
}

napi_value Start(napi_env env, napi_callback_info info) {
  size_t argc = 1;
  napi_value args[1];
  napi_get_cb_info(env, info, &argc, args, nullptr, nullptr);
  if (capture && capture->running) { napi_value ok; napi_get_boolean(env, true, &ok); return ok; }
  if (argc < 1) { napi_throw_type_error(env, nullptr, "callback gerekli"); return nullptr; }

  delete capture;
  capture = new Capture();
  napi_value name;
  napi_create_string_utf8(env, "loopback", NAPI_AUTO_LENGTH, &name);
  if (napi_create_threadsafe_function(env, args[0], nullptr, name, 0, 1, nullptr, nullptr, nullptr, CallJs,
                                      &capture->emit) != napi_ok) {
    napi_throw_error(env, nullptr, "threadsafe function olusturulamadi");
    return nullptr;
  }
  napi_unref_threadsafe_function(env, capture->emit);
  capture->running = true;
  capture->worker = std::thread(CaptureLoop);

  napi_value ok;
  napi_get_boolean(env, true, &ok);
  return ok;
}

napi_value Stop(napi_env env, napi_callback_info /*info*/) {
  if (capture) {
    capture->running = false;
    if (capture->worker.joinable()) capture->worker.join();
    if (capture->emit) { napi_release_threadsafe_function(capture->emit, napi_tsfn_release); capture->emit = nullptr; }
  }
  napi_value ok;
  napi_get_boolean(env, true, &ok);
  return ok;
}

napi_value LastError(napi_env env, napi_callback_info /*info*/) {
  napi_value message;
  napi_create_string_utf8(env, capture && !capture->error.empty() ? capture->error.c_str() : "", NAPI_AUTO_LENGTH, &message);
  return message;
}

}  // namespace

#else   // Windows dışı: modül var ama çalışmaz
namespace {
napi_value Start(napi_env env, napi_callback_info) { napi_value ok; napi_get_boolean(env, false, &ok); return ok; }
napi_value Stop(napi_env env, napi_callback_info) { napi_value ok; napi_get_boolean(env, false, &ok); return ok; }
napi_value LastError(napi_env env, napi_callback_info) {
  napi_value message; napi_create_string_utf8(env, "yalnizca Windows", NAPI_AUTO_LENGTH, &message); return message;
}
}  // namespace
#endif

napi_value Init(napi_env env, napi_value exports) {
  napi_value start, stop, error;
  napi_create_function(env, "start", NAPI_AUTO_LENGTH, Start, nullptr, &start);
  napi_create_function(env, "stop", NAPI_AUTO_LENGTH, Stop, nullptr, &stop);
  napi_create_function(env, "lastError", NAPI_AUTO_LENGTH, LastError, nullptr, &error);
  napi_set_named_property(env, exports, "start", start);
  napi_set_named_property(env, exports, "stop", stop);
  napi_set_named_property(env, exports, "lastError", error);
  return exports;
}

NAPI_MODULE(NODE_GYP_MODULE_NAME, Init)
