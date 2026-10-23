#include "ipc_client.h"
#include "log.h"
#include <stdio.h>

bool IpcClient::open() {
  pid_ = GetCurrentProcessId();
  wchar_t name[128];
  swprintf(name, 128, L"Local\\P2Overlay_Frame_%lu", pid_);
  mapping_ = OpenFileMappingW(FILE_MAP_ALL_ACCESS, FALSE, name);
  if (!mapping_) { return false; } // host henüz hazır değil; sonra tekrar denenir
  base_ = (uint8_t*)MapViewOfFile(mapping_, FILE_MAP_ALL_ACCESS, 0, 0, p2_total_mapping_bytes());
  if (!base_) { CloseHandle(mapping_); mapping_ = nullptr; return false; }
  header_ = (P2OverlayHeader*)base_;
  if (header_->magic != P2_OVL_MAGIC || header_->schema != P2_OVL_SCHEMA) {
    p2log("paylasimli bellek semasi uyumsuz (magic=%08x schema=%u)", header_->magic, header_->schema);
    close(); return false;
  }
  swprintf(name, 128, L"Local\\P2Overlay_FrameReady_%lu", pid_);
  frameReady_ = OpenEventW(SYNCHRONIZE, FALSE, name);
  swprintf(name, 128, L"Local\\P2Overlay_Lock_%lu", pid_);
  lock_ = OpenMutexW(SYNCHRONIZE | MUTEX_MODIFY_STATE, FALSE, name);
  wchar_t pipeName[128];
  swprintf(pipeName, 128, L"\\\\.\\pipe\\P2Overlay_Input_%lu", pid_);
  // Boru host tarafında sunucu; birkaç kez dene.
  for (int i = 0; i < 20; i++) {
    pipe_ = CreateFileW(pipeName, GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr);
    if (pipe_ != INVALID_HANDLE_VALUE) break;
    if (GetLastError() == ERROR_PIPE_BUSY) WaitNamedPipeW(pipeName, 500);
    else Sleep(100);
  }
  p2log("IPC baglandi (pipe=%s)", pipe_ != INVALID_HANDLE_VALUE ? "ok" : "yok");
  return true;
}

void IpcClient::close() {
  if (base_) { UnmapViewOfFile(base_); base_ = nullptr; }
  if (mapping_) { CloseHandle(mapping_); mapping_ = nullptr; }
  if (frameReady_) { CloseHandle(frameReady_); frameReady_ = nullptr; }
  if (lock_) { CloseHandle(lock_); lock_ = nullptr; }
  if (pipe_ != INVALID_HANDLE_VALUE) { CloseHandle(pipe_); pipe_ = INVALID_HANDLE_VALUE; }
  header_ = nullptr;
}

const uint8_t* IpcClient::frontSlot() {
  if (!header_) return nullptr;
  return base_ + p2_frame_slot_offset(header_->frontIndex & 1);
}
uint8_t* IpcClient::shotSlot() {
  if (!header_) return nullptr;
  return base_ + p2_shot_slot_offset();
}
P2FrameView IpcClient::frameView() {
  P2FrameView v;
  if (!header_) return v;
  v.id = header_->frameId; v.width = header_->width; v.height = header_->height; v.stride = header_->stride;
  v.dx = header_->dirtyX; v.dy = header_->dirtyY; v.dw = header_->dirtyW; v.dh = header_->dirtyH;
  v.pixels = base_ + p2_frame_slot_offset(header_->frontIndex & 1);
  return v;
}
void IpcClient::ackFrame(uint64_t id) {
  if (header_) header_->reserved2[P2_OVL_ACK_FRAME_SLOT] = (uint32_t)id;
}
bool IpcClient::frameChanged(uint64_t& lastId) {
  if (!header_) return false;
  uint64_t id = header_->frameId;
  if (id == lastId) return false;
  lastId = id; return true;
}
void IpcClient::sendInput(const P2InputMsg& msg) {
  if (pipe_ == INVALID_HANDLE_VALUE) return;
  DWORD written = 0;
  WriteFile(pipe_, &msg, sizeof(msg), &written, nullptr);
}
bool IpcClient::lockHeader(DWORD timeoutMs) {
  if (!lock_) return false; // mutex açılamadıysa kilitsiz devam edilir (okuma yine de güvenli: alanlar 4 baytlık)
  DWORD r = WaitForSingleObject(lock_, timeoutMs);
  return r == WAIT_OBJECT_0 || r == WAIT_ABANDONED;
}
void IpcClient::unlockHeader() { if (lock_) ReleaseMutex(lock_); }
