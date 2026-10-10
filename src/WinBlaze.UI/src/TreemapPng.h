#pragma once
#include <windows.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <cstdint>
#include <limits>
#include <vector>
#include <new>
namespace WinBlaze::UI::Treemap {
    // Serializes the rendered treemap's BGRA pixels, independent of window chrome.
    inline HRESULT SavePng(const wchar_t* path, UINT width, UINT height, UINT stride, const uint8_t* pixels, size_t bytes, bool grayscale = false) {
        if (!path || !pixels || width==0 || height==0 || width>8192 || height>8192 || stride<width*4 || bytes<static_cast<size_t>(stride)*height || bytes>UINT_MAX) { return E_INVALIDARG; }
        std::vector<uint8_t> gray;
        if (grayscale) {
            try { gray.assign(pixels, pixels + static_cast<size_t>(stride)*height); }
            catch (std::bad_alloc const&) { return E_OUTOFMEMORY; }
            for (UINT y=0;y<height;++y) for (UINT x=0;x<width;++x) {
                auto pixel=gray.data()+static_cast<size_t>(y)*stride+static_cast<size_t>(x)*4;
                const auto light=static_cast<uint8_t>((29u*pixel[0]+150u*pixel[1]+77u*pixel[2]+128u)>>8);
                pixel[0]=pixel[1]=pixel[2]=light;
            }
            pixels=gray.data();
            bytes=gray.size();
        }
        using Microsoft::WRL::ComPtr;
        ComPtr<IWICImagingFactory> factory;
        HRESULT hr=CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&factory));
        if (FAILED(hr)) { return hr; }
        ComPtr<IWICStream> stream;
        if (FAILED(hr=factory->CreateStream(&stream))) {return hr;}
        if (FAILED(hr=stream->InitializeFromFilename(path,GENERIC_WRITE))) {return hr;}
        ComPtr<IWICBitmapEncoder> encoder;
        if (FAILED(hr=factory->CreateEncoder(GUID_ContainerFormatPng,nullptr,&encoder))) {return hr;}
        if (FAILED(hr=encoder->Initialize(stream.Get(),WICBitmapEncoderNoCache))) {return hr;}
        ComPtr<IWICBitmapFrameEncode> frame;
        if (FAILED(hr=encoder->CreateNewFrame(&frame,nullptr))) {return hr;}
        if (FAILED(hr=frame->Initialize(nullptr))) {return hr;}
        if (FAILED(hr=frame->SetSize(width,height))) {return hr;}
        WICPixelFormatGUID format=GUID_WICPixelFormat32bppBGRA;
        if (FAILED(hr=frame->SetPixelFormat(&format))) {return hr;}
        if (format!=GUID_WICPixelFormat32bppBGRA) {return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;}
        if (FAILED(hr=frame->WritePixels(height,stride,static_cast<UINT>(bytes),const_cast<BYTE*>(pixels)))) {return hr;}
        if (FAILED(hr=frame->Commit())) {return hr;}
        return encoder->Commit();
    }
}
