#pragma once
#include <windows.h>
#include <PortableDeviceApi.h>
#include <PortableDevice.h>
#include <wrl/client.h>
#include <string>
#include <vector>
#include <stdexcept>
#include <unordered_set>
#include <unordered_map>
#include <limits>
#include <fstream>
#include <filesystem>
#include <atomic>
#include <functional>
#pragma comment(lib, "PortableDeviceGUIDs.lib")
#pragma comment(lib, "ole32.lib")
namespace WinBlaze::Portable {
inline void Check(HRESULT value) {
    if(FAILED(value))throw std::runtime_error("Windows portable-device API failed: "+std::to_string(static_cast<unsigned long>(value)));
}
struct Device {std::wstring id,name;};
inline std::vector<Device> Devices() {
    Microsoft::WRL::ComPtr<IPortableDeviceManager> manager;
    Check(CoCreateInstance(CLSID_PortableDeviceManager,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&manager)));
    DWORD count{};
    Check(manager->GetDevices(nullptr,&count));
    if(count>4096)throw std::runtime_error("Too many portable devices");
    std::vector<LPWSTR> ids(count);
    if(count)Check(manager->GetDevices(ids.data(),&count));
    std::vector<Device> result;
    try {
        for(DWORD index=0;index<count;++index) {
            Device device;device.id=ids[index];
            DWORD length{};
            if(SUCCEEDED(manager->GetDeviceFriendlyName(ids[index],nullptr,&length))&&length&&length<=65536) {
                std::vector<wchar_t> name(length);
                if(SUCCEEDED(manager->GetDeviceFriendlyName(ids[index],name.data(),&length)))device.name=name.data();
            }
            if(device.name.empty())device.name=L"Portable device";
            result.push_back(std::move(device));
        }
    } catch(...) {for(auto id:ids)CoTaskMemFree(id);throw;}
    for(auto id:ids)CoTaskMemFree(id);
    return result;
}

struct Node {
    std::wstring id,path;
    size_t parent{};
    bool directory{};
    uint64_t bytes{},files{},directories{};
};
struct Scan {
    std::vector<Node> nodes;
    uint64_t unknown_sizes{},errors{};
};
inline std::wstring SafeName(std::wstring name) {
    for(auto& ch:name)if(ch<32||std::wstring(L"\\/:*?\"<>|").find(ch)!=std::wstring::npos)ch=L'_';
    while(!name.empty()&&(name.back()==L'.'||name.back()==L' '))name.pop_back();
    if(name.empty()||name==L"."||name==L"..")name=L"Unnamed";
    return name;
}
inline std::wstring Value(IPortableDeviceValues* values,REFPROPERTYKEY key) {
    LPWSTR text{};
    if(FAILED(values->GetStringValue(key,&text)))return {};
    std::wstring result=text?text:L"";
    CoTaskMemFree(text);return result;
}
inline Scan ScanDevice(Device const& selected,std::atomic_bool const* cancel=nullptr) {
    Microsoft::WRL::ComPtr<IPortableDevice> device;
    Check(CoCreateInstance(CLSID_PortableDeviceFTM,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&device)));
    Microsoft::WRL::ComPtr<IPortableDeviceValues> client;
    Check(CoCreateInstance(CLSID_PortableDeviceValues,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&client)));
    Check(client->SetStringValue(WPD_CLIENT_NAME,L"WinBlaze"));
    Check(client->SetUnsignedIntegerValue(WPD_CLIENT_MAJOR_VERSION,1));
    Check(client->SetUnsignedIntegerValue(WPD_CLIENT_MINOR_VERSION,0));
    Check(client->SetUnsignedIntegerValue(WPD_CLIENT_REVISION,0));
    Check(client->SetUnsignedIntegerValue(WPD_CLIENT_DESIRED_ACCESS,GENERIC_READ));
    Check(device->Open(selected.id.c_str(),client.Get()));
    struct Close {IPortableDevice* device;~Close(){device->Close();}} close{device.Get()};
    Microsoft::WRL::ComPtr<IPortableDeviceContent> content;
    Check(device->Content(&content));
    Microsoft::WRL::ComPtr<IPortableDeviceProperties> properties;
    Check(content->Properties(&properties));
    Microsoft::WRL::ComPtr<IPortableDeviceKeyCollection> keys;
    Check(CoCreateInstance(CLSID_PortableDeviceKeyCollection,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&keys)));
    for(auto const& key:{WPD_OBJECT_NAME,WPD_OBJECT_ORIGINAL_FILE_NAME,WPD_OBJECT_CONTENT_TYPE,WPD_OBJECT_SIZE})Check(keys->Add(key));
    Scan result;
    result.nodes.push_back({WPD_DEVICE_OBJECT_ID,L"MTP:\\"+SafeName(selected.name)+L"-"+
        std::to_wstring(std::hash<std::wstring>{}(selected.id)),0,true,0,0,0});
    std::unordered_set<std::wstring> ids{WPD_DEVICE_OBJECT_ID};
    for(size_t parent=0;parent<result.nodes.size();++parent) {
        if(cancel&&cancel->load())throw std::runtime_error("Portable-device scan canceled");
        if(!result.nodes[parent].directory)continue;
        Microsoft::WRL::ComPtr<IEnumPortableDeviceObjectIDs> objects;
        auto status=content->EnumObjects(0,result.nodes[parent].id.c_str(),nullptr,&objects);
        if(FAILED(status)){if(parent==0)Check(status);++result.errors;continue;}
        std::unordered_set<std::wstring> names;
        for(;;) {
            if(cancel&&cancel->load())throw std::runtime_error("Portable-device scan canceled");
            LPWSTR id{};DWORD fetched{};
            status=objects->Next(1,&id,&fetched);
            if(FAILED(status)){CoTaskMemFree(id);Check(status);}
            if(!fetched){CoTaskMemFree(id);break;}
            std::wstring object=id;CoTaskMemFree(id);
            if(!ids.insert(object).second){++result.errors;continue;}
            Microsoft::WRL::ComPtr<IPortableDeviceValues> values;
            if(FAILED(properties->GetValues(object.c_str(),keys.Get(),&values))){++result.errors;continue;}
            GUID type{};
            if(FAILED(values->GetGuidValue(WPD_OBJECT_CONTENT_TYPE,&type))){++result.errors;continue;}
            auto name=Value(values.Get(),WPD_OBJECT_ORIGINAL_FILE_NAME);
            if(name.empty())name=Value(values.Get(),WPD_OBJECT_NAME);
            name=SafeName(name);
            auto folded=name;
            for(auto& ch:folded)ch=static_cast<wchar_t>(towlower(ch));
            const auto base=name;
            size_t suffix=1;
            while(!names.insert(folded).second) {
                name=base+L"-"+std::to_wstring(suffix++);
                folded=name;
                for(auto& ch:folded)ch=static_cast<wchar_t>(towlower(ch));
            }
            const bool folder=IsEqualGUID(type,WPD_CONTENT_TYPE_FOLDER)||IsEqualGUID(type,WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT);
            ULONGLONG bytes{};
            if(!folder&&FAILED(values->GetUnsignedLargeIntegerValue(WPD_OBJECT_SIZE,&bytes)))++result.unknown_sizes;
            result.nodes.push_back({object,result.nodes[parent].path+L"\\"+name,parent,folder,bytes,folder?0ull:1ull,0});
            if(result.nodes.size()>1000000)throw std::runtime_error("Portable-device catalog exceeds one million objects");
        }
    }
    for(size_t index=result.nodes.size();index-->1;) {
        auto const& node=result.nodes[index];auto& parent=result.nodes[node.parent];
        if((std::numeric_limits<uint64_t>::max)()-parent.bytes<node.bytes)throw std::runtime_error("Portable-device size overflow");
        parent.bytes+=node.bytes;parent.files+=node.files;
        parent.directories+=node.directories+(node.directory?1:0);
    }
    return result;
}
inline std::string Utf8(std::wstring const& value) {
    if(value.empty())return {};
    auto count=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,value.data(),static_cast<int>(value.size()),nullptr,0,nullptr,nullptr);
    if(!count)throw std::runtime_error("Invalid portable-device name");
    std::string result(count,'\0');
    if(!WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,value.data(),static_cast<int>(value.size()),result.data(),count,nullptr,nullptr))throw std::runtime_error("Invalid portable-device name");
    return result;
}
inline void WriteCsv(Scan const& scan,std::filesystem::path const& output) {
    if(scan.nodes.empty())throw std::runtime_error("No portable scan records");
    std::ofstream stream(output,std::ios::binary);
    if(!stream)throw std::runtime_error("Cannot write portable scan snapshot");
    stream<<"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n";
    for(auto const& node:scan.nodes) {
        const auto path=Utf8(node.path);stream<<'"';
        for(auto ch:path){if(ch=='"')stream<<'"';stream<<ch;}
        stream<<"\","<<(node.directory?"Directory":"File")<<','<<node.bytes<<",0,"<<node.files<<','<<node.directories<<",\r\n";
    }
    if(!stream)throw std::runtime_error("Portable snapshot write failed");
}
}
