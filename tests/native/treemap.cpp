#include "../../src/WinBlaze.UI/src/TreemapGeometry.h"
#include "../../src/WinBlaze.UI/src/TreemapPng.h"
#include <cmath>
#include <filesystem>
#include <vector>
#include <iostream>
#include <stdexcept>
int main() {
    using namespace WinBlaze::UI::Treemap;
    const Rect root{0,0,0.5f,0.25f};
    const auto parts = Split(root,1,4);
    auto area=[](Rect r){return (r.right-r.left)*(r.bottom-r.top);};
    if (!Visible(parts.first) || !Visible(parts.second) || std::abs(area(parts.first)/area(root)-0.25f)>0.00001f) throw std::runtime_error("Small positive tiles lost or distorted");
    const auto strips = SplitHorizontal({0,0,100,50}, 1, 4);
    if (strips.first.right != 25 || strips.first.bottom != 50 ||
        strips.second.left != 25 || area(strips.first) != 1250) throw std::runtime_error("Strip layout distorts weights");

    const auto space = VolumeSpace(100,20,50);
    if (!space.valid || space.unaccounted != 30 || space.free != 20 || space.scanned != 50)
        throw std::runtime_error("Free or unaccounted space incorrect");
    if (VolumeSpace(100,101,0).valid || VolumeSpace(100,20,81).valid)
        throw std::runtime_error("Inconsistent volume totals accepted");

    if (Visible({0,0,0,1})) throw std::runtime_error("Zero-area rectangle visible");
    if (RemainingBytes(100,{20,30})!=50 || RemainingBytes(10,{20})!=0) throw std::runtime_error("Omitted child weights lost");
    const std::vector<HierarchyNode> hierarchy{{0, SIZE_MAX, 100}, {1, 0, 25}, {2, 0, 75}, {3, 2, 75}};
    const auto flame = Hierarchy(hierarchy, 100, 60, false);
    if(flame.size()!=4 || std::abs(flame[1].bounds.right-25)>0.001 || flame[3].depth!=2)
        throw std::runtime_error("Flame hierarchy must retain ancestry and weighted widths");
    const auto radial = Hierarchy(hierarchy, 100, 100, true);
    if(radial.size()!=4 || std::abs(radial[1].end-radial[1].start-1.57079632679)>0.001)
        throw std::runtime_error("Sunburst child angles must preserve weights");
    if(!Contains(radial[1],60,75) || Contains(radial[1],25,50) || Contains(radial[1],50,50))
        throw std::runtime_error("Sunburst hit testing must use annular sectors");
    if(!Hierarchy({},100,100,true).empty() || !Hierarchy(hierarchy,0,100,true).empty())
        throw std::runtime_error("Empty or zero-size hierarchy must not render");
    CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    const std::filesystem::path output = std::filesystem::temp_directory_path() / L"winblaze-treemap-test.png";
    std::vector<uint8_t> pixels(8*4*4,255);
    for (size_t i=0;i<pixels.size();i+=4) {pixels[i]=10; pixels[i+1]=20; pixels[i+2]=30;}
    if (FAILED(SavePng(output.c_str(),8,4,32,pixels.data(),pixels.size()))) throw std::runtime_error("PNG encode failed");
    Microsoft::WRL::ComPtr<IWICImagingFactory> factory;
    if (FAILED(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&factory)))) throw std::runtime_error("WIC unavailable");
    Microsoft::WRL::ComPtr<IWICBitmapDecoder> decoder;
    if (FAILED(factory->CreateDecoderFromFilename(output.c_str(),nullptr,GENERIC_READ,WICDecodeMetadataCacheOnLoad,&decoder))) throw std::runtime_error("PNG not decodable");
    Microsoft::WRL::ComPtr<IWICBitmapFrameDecode> frame; decoder->GetFrame(0,&frame);
    UINT w=0,h=0; frame->GetSize(&w,&h);
    Microsoft::WRL::ComPtr<IWICFormatConverter> converter; factory->CreateFormatConverter(&converter);
    converter->Initialize(frame.Get(),GUID_WICPixelFormat32bppBGRA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom);
    std::vector<uint8_t> decoded(pixels.size()); converter->CopyPixels(nullptr,32,static_cast<UINT>(decoded.size()),decoded.data());
    if (w!=8 || h!=4 || decoded!=pixels) throw std::runtime_error("PNG dimensions/colors differ");
    if (SUCCEEDED(SavePng(output.c_str(),8,4,1,pixels.data(),pixels.size()))) throw std::runtime_error("Invalid stride accepted");
    frame.Reset(); decoder.Reset(); converter.Reset();
    if (FAILED(SavePng(output.c_str(),8,4,32,pixels.data(),pixels.size(),true))) throw std::runtime_error("Grayscale PNG encode failed");
    factory->CreateDecoderFromFilename(output.c_str(),nullptr,GENERIC_READ,WICDecodeMetadataCacheOnLoad,&decoder);
    decoder->GetFrame(0,&frame);factory->CreateFormatConverter(&converter);
    converter->Initialize(frame.Get(),GUID_WICPixelFormat32bppBGRA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom);
    converter->CopyPixels(nullptr,32,static_cast<UINT>(decoded.size()),decoded.data());
    for(size_t i=0;i<decoded.size();i+=4) {
        if(decoded[i]!=decoded[i+1]||decoded[i]!=decoded[i+2]||decoded[i+3]!=pixels[i+3])throw std::runtime_error("Grayscale PNG channels or alpha differ");
    }
    frame.Reset(); decoder.Reset(); converter.Reset(); factory.Reset();
    std::filesystem::remove(output); CoUninitialize();
    std::cout << "PASS: subpixel tiles preserve area; omitted children retain their weight\n";
}
