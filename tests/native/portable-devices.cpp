
#include "../../src/WinBlaze.UI/src/PortableDevices.h"
#include <iostream>
int wmain(int argc,wchar_t** argv) {
    auto initialized=CoInitializeEx(nullptr,COINIT_MULTITHREADED);
    if(FAILED(initialized))return 1;
    try {
        const auto devices=WinBlaze::Portable::Devices();
        auto json_string=[](std::ofstream& out,std::wstring const& value){
            out<<'"';
            for(unsigned char ch:WinBlaze::Portable::Utf8(value)) {
                if(ch=='"'||ch=='\\')out<<'\\';
                if(ch<32)out<<"\\u00"<<"0123456789abcdef"[ch>>4]<<"0123456789abcdef"[ch&15];
                else out<<static_cast<char>(ch);
            }
            out<<'"';
        };
        if(argc==3 && std::wstring(argv[1])==L"--inventory-json") {
            std::ofstream out(std::filesystem::path(argv[2]),std::ios::binary);
            out<<'[';
            bool separator=false;
            for(auto const& device:devices){if(separator)out<<',';separator=true;out<<"{\"id\":";json_string(out,device.id);out<<",\"name\":";json_string(out,device.name);out<<'}';}
            out<<"]\n";CoUninitialize();return out?0:5;
        }
        if(argc==4 && std::wstring(argv[1])==L"--scan") {
            const auto selected=std::find_if(devices.begin(),devices.end(),[&](auto const& device){return device.id==argv[2];});
            if(selected==devices.end()){CoUninitialize();return 6;}
            const auto scan=WinBlaze::Portable::ScanDevice(*selected);
            WinBlaze::Portable::WriteCsv(scan,std::filesystem::path(argv[3]));
            std::cout<<"{\"files\":"<<scan.nodes[0].files<<",\"logical_bytes\":"<<scan.nodes[0].bytes<<",\"errors\":"<<scan.errors<<",\"unknown_sizes\":"<<scan.unknown_sizes<<"}\n";
            CoUninitialize();return 0;
        }
        if(argc!=1){CoUninitialize();return 2;}

        for(auto const& device:devices) {
            if(device.id.empty()||device.name.empty())return 2;
            std::wcout<<device.name<<L"\n";
        }
        bool rejected=false;
        try { WinBlaze::Portable::ScanDevice({L"WinBlaze-owned-invalid-device",L"Invalid"}); }
        catch(...) { rejected=true; }
        if(!rejected)return 4;
        std::cout<<"PASS: real WPD inventory is read without opening device content\n";
        CoUninitialize();return 0;
    } catch(...) {CoUninitialize();return 3;}
}
