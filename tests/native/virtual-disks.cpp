
#include "../../src/WinBlaze.UI/src/VirtualDisks.h"
#include <filesystem>
#include <fstream>
#include <iostream>
int wmain(int argc,wchar_t** argv) {
    if(argc!=2&&(argc!=3||std::wstring(argv[2])!=L"--create"))return 1;
    const auto path=std::filesystem::absolute(argv[1]);
    if(std::filesystem::exists(path))return 2;
    {std::ofstream file(path,std::ios::binary);file<<"owned invalid virtual disk";}
    const auto before=std::filesystem::file_size(path);
    const auto status=WinBlaze::VirtualDisks::Compact(path.wstring());
    if(status==ERROR_SUCCESS||std::filesystem::file_size(path)!=before)return 3;
    std::filesystem::remove(path);
    {
        const auto valid=path.wstring()+L".valid.vhdx";
        VIRTUAL_STORAGE_TYPE type{};
        type.DeviceId=VIRTUAL_STORAGE_TYPE_DEVICE_VHDX;
        type.VendorId={0xec984aec,0xa0f9,0x47e9,{0x90,0x1f,0x71,0x41,0x5a,0x66,0x34,0x5b}};
        CREATE_VIRTUAL_DISK_PARAMETERS create{};create.Version=CREATE_VIRTUAL_DISK_VERSION_1;
        create.Version1.MaximumSize=16ull*1024*1024;
        create.Version1.BlockSizeInBytes=2*1024*1024;create.Version1.SectorSizeInBytes=512;
        HANDLE handle{};
        const auto created=CreateVirtualDisk(&type,valid.c_str(),VIRTUAL_DISK_ACCESS_ALL,nullptr,
            CREATE_VIRTUAL_DISK_FLAG_NONE,0,&create,nullptr,&handle);
        if(created) {std::cout<<"PENDING: owned VHDX creation failed with Windows status "<<created<<"\n";return 77;}
        CloseHandle(handle);
        if(argc==3) {std::cout<<"CREATED: owned empty VHDX fixture\n";return 0;}
        const auto oldSize=std::filesystem::file_size(valid);
        const auto compacted=WinBlaze::VirtualDisks::Compact(valid);
        if(compacted) {std::cout<<"FAIL: empty VHDX compaction status "<<compacted<<"\n";return 4;}
        if(std::filesystem::file_size(valid)>oldSize)return 5;
        std::filesystem::remove(valid);
        std::cout<<"PASS: real empty dynamic VHDX compacts without mounting or growing\n";
    }
    std::cout<<"PASS: invalid virtual-disk content is refused without changing the owned file\n";
}
