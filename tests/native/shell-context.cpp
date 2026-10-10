
#include "../../src/WinBlaze.UI/src/ShellContext.h"
#include <filesystem>
#include <fstream>
#include <iostream>
int wmain(int argc,wchar_t** argv) {
    if(argc!=2)return 2;
    auto initialized=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
    if(FAILED(initialized))return 3;
    std::filesystem::path root(argv[1]);std::filesystem::create_directories(root);
    std::ofstream(root/L"file.txt")<<"unchanged";
    {
        WinBlaze::Shell::Context file;
        if(FAILED(file.Build((root/L"file.txt").wstring(),nullptr)))return 4;
        auto verbs=file.Verbs();
        if(std::find(verbs.begin(),verbs.end(),"properties")==verbs.end()||
           std::find(verbs.begin(),verbs.end(),"rename")==verbs.end())return 5;
        WinBlaze::Shell::Context folder;
        if(FAILED(folder.Build(root.wstring(),nullptr)))return 6;
        auto folder_verbs=folder.Verbs();
        if(std::find(folder_verbs.begin(),folder_verbs.end(),"properties")==folder_verbs.end())return 7;
        WinBlaze::Shell::Context missing;
        if(SUCCEEDED(missing.Build((root/L"missing").wstring(),nullptr)))return 8;
    }
    std::ifstream input(root/L"file.txt");std::string text;input>>text;input.close();
    if(text!="unchanged")return 9;
    std::filesystem::remove(root/L"file.txt");std::filesystem::remove(root);
    CoUninitialize();std::cout<<"PASS: real Explorer file/folder verbs available without changing contents\n";
}
