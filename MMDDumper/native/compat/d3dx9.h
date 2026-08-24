#pragma once

#include <d3d9.h>

struct D3DXMACRO {
  LPCSTR Name;
  LPCSTR Definition;
};

struct D3DXEFFECT_DESC {
  UINT Parameters;
  UINT Techniques;
  UINT Functions;
  UINT Pools;
};

struct D3DXPARAMETER_DESC;
struct D3DXTECHNIQUE_DESC;
struct D3DXPASS_DESC;
struct D3DXFUNCTION_DESC;
struct D3DXVECTOR4 {
  FLOAT x;
  FLOAT y;
  FLOAT z;
  FLOAT w;
};
using D3DXMATRIX = D3DMATRIX;
using D3DXHANDLE = LPCSTR;

struct ID3DXInclude;
struct ID3DXEffect;
struct ID3DXEffectPool;
struct ID3DXBuffer;
struct ID3DXEffectStateManager;

using LPD3DXINCLUDE = ID3DXInclude*;
using LPD3DXEFFECT = ID3DXEffect*;
using LPD3DXEFFECTPOOL = ID3DXEffectPool*;
using LPD3DXBUFFER = ID3DXBuffer*;
using LPD3DXEFFECTSTATEMANAGER = ID3DXEffectStateManager*;
