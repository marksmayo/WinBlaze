#pragma once
#include <windows.h>
#include <virtdisk.h>
#include <filesystem>
#include <string>
#include <cwctype>
#pragma comment(lib,"VirtDisk.lib")
namespace WinBlaze::VirtualDisks {
// Zero-block compaction only. Never mounts, detaches, formats, or shrinks a disk.
inline DWORD Compact(std::wstring const& input) {
    if(input.empty()||input.find(L'\0')!=std::wstring::npos)return ERROR_INVALID_NAME;
    std::error_code error;
    auto path=std::filesystem::absolute(input,error);
    if(error)return ERROR_INVALID_NAME;
    auto extension=path.extension().wstring();
    for(auto& ch:extension)ch=static_cast<wchar_t>(towlower(ch));
    if(extension!=L".vhd"&&extension!=L".vhdx")return ERROR_INVALID_PARAMETER;
    const auto attributes=GetFileAttributesW(path.c_str());
    if(attributes==INVALID_FILE_ATTRIBUTES)return GetLastError();
    if(attributes&(FILE_ATTRIBUTE_DIRECTORY|FILE_ATTRIBUTE_REPARSE_POINT|FILE_ATTRIBUTE_OFFLINE|
        FILE_ATTRIBUTE_READONLY|FILE_ATTRIBUTE_ENCRYPTED|FILE_ATTRIBUTE_COMPRESSED))return ERROR_NOT_SUPPORTED;
    VIRTUAL_STORAGE_TYPE type{};
    type.DeviceId=extension==L".vhdx"?VIRTUAL_STORAGE_TYPE_DEVICE_VHDX:VIRTUAL_STORAGE_TYPE_DEVICE_VHD;
    // VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT from the Windows SDK.
    type.VendorId={0xec984aec,0xa0f9,0x47e9,{0x90,0x1f,0x71,0x41,0x5a,0x66,0x34,0x5b}};
    HANDLE handle{};
    auto status=OpenVirtualDisk(&type,path.c_str(),
        static_cast<VIRTUAL_DISK_ACCESS_MASK>(VIRTUAL_DISK_ACCESS_METAOPS|VIRTUAL_DISK_ACCESS_GET_INFO),
        OPEN_VIRTUAL_DISK_FLAG_NONE,nullptr,&handle);
    if(status)return status;
    struct Close{HANDLE handle;~Close(){CloseHandle(handle);}} close{handle};
    GET_VIRTUAL_DISK_INFO info{};info.Version=GET_VIRTUAL_DISK_INFO_IS_LOADED;
    ULONG bytes=sizeof(info);
    status=GetVirtualDiskInformation(handle,&bytes,&info,nullptr);
    if(status)return status;
    if(info.IsLoaded)return ERROR_BUSY;
    return CompactVirtualDisk(handle,COMPACT_VIRTUAL_DISK_FLAG_NONE,nullptr,nullptr);
}
}
