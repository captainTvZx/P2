// D3D11 shader derleme: overlay quad için minik VS/PS. d3dcompiler ile runtime
// derlenir (D3DCompile). Üretimde önceden derlenmiş bytecode gömmek daha hızlıdır.
#include <windows.h>
#include <d3d11.h>
#include <d3dcompiler.h>
#include "log.h"
#pragma comment(lib, "d3dcompiler.lib")

static const char* kVS =
  "struct VSOut{float4 pos:SV_POSITION;float2 uv:TEXCOORD0;};"
  "VSOut main(uint id:SV_VertexID){VSOut o;float2 uv=float2((id<<1)&2,id&2);"
  "o.uv=uv;o.pos=float4(uv.x*2-1,1-uv.y*2,0,1);return o;}";
static const char* kPS =
  "Texture2D t:register(t0);SamplerState s:register(s0);"
  "float4 main(float4 pos:SV_POSITION,float2 uv:TEXCOORD0):SV_TARGET{return t.Sample(s,uv);}";

bool compileShaders(ID3D11Device* dev, ID3D11VertexShader** vs, ID3D11PixelShader** ps) {
  ID3DBlob* vsb = nullptr; ID3DBlob* psb = nullptr; ID3DBlob* err = nullptr;
  if (FAILED(D3DCompile(kVS, strlen(kVS), nullptr, nullptr, nullptr, "main", "vs_4_0", 0, 0, &vsb, &err))) { if (err) err->Release(); return false; }
  if (FAILED(D3DCompile(kPS, strlen(kPS), nullptr, nullptr, nullptr, "main", "ps_4_0", 0, 0, &psb, &err))) { if (err) err->Release(); vsb->Release(); return false; }
  bool ok = SUCCEEDED(dev->CreateVertexShader(vsb->GetBufferPointer(), vsb->GetBufferSize(), nullptr, vs))
         && SUCCEEDED(dev->CreatePixelShader(psb->GetBufferPointer(), psb->GetBufferSize(), nullptr, ps));
  vsb->Release(); psb->Release();
  return ok;
}

// D3D12 icin ham bytecode (sm 5_0). Cagiran blob'lari serbest birakir.
bool compileShaderBlobs(ID3DBlob** vs, ID3DBlob** ps) {
  ID3DBlob* err = nullptr;
  if (FAILED(D3DCompile(kVS, strlen(kVS), nullptr, nullptr, nullptr, "main", "vs_5_0", 0, 0, vs, &err))) { if (err) err->Release(); return false; }
  if (FAILED(D3DCompile(kPS, strlen(kPS), nullptr, nullptr, nullptr, "main", "ps_5_0", 0, 0, ps, &err))) { if (err) err->Release(); (*vs)->Release(); *vs = nullptr; return false; }
  return true;
}
