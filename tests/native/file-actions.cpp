#include "../../src/WinBlaze.UI/src/FileActions.h"
#include <fstream>
#include <filesystem>
#include <iostream>
int wmain(int argc,wchar_t** argv){
 if(argc!=2)return 2;std::filesystem::path root(argv[1]);std::filesystem::create_directories(root);
 auto source=root/L"source.txt",destination=root/L"moved.txt",existing=root/L"existing.txt";
 std::ofstream(source)<<"source bytes";std::ofstream(existing)<<"existing bytes";
 if(WinBlaze::Actions::Move(source.wstring(),existing.wstring())==0||!std::filesystem::exists(source))return 1;
 if(WinBlaze::Actions::Move(source.wstring(),destination.wstring())!=0||std::filesystem::exists(source)||!std::filesystem::exists(destination))return 3;
 if(WinBlaze::Actions::Recycle(root.wstring())==0)return 4;
 if(WinBlaze::Actions::Recycle(destination.wstring())!=0||std::filesystem::exists(destination))return 5;
 auto directory=root/L"old folder",renamed=root/L"new folder";
 std::filesystem::create_directories(directory);
 std::ofstream(directory/L"child.txt")<<"retained child";
 if(WinBlaze::Actions::Rename(directory.wstring(),L"../escape")==0)return 6;
 if(WinBlaze::Actions::Rename(directory.wstring(),L"new folder")!=0||std::filesystem::exists(directory)||!std::filesystem::exists(renamed/L"child.txt"))return 7;
 if(WinBlaze::Actions::Rename(existing.wstring(),L"new folder")==0||!std::filesystem::exists(existing))return 8;
 std::filesystem::remove(renamed/L"child.txt");std::filesystem::remove(renamed);
 auto batch=root/L"batch",target=root/L"target";
 std::filesystem::create_directories(batch/L"folder");std::filesystem::create_directories(target);
 std::ofstream(batch/L"file.txt")<<"file";std::ofstream(batch/L"folder/child.txt")<<"child";
 std::ofstream(target/L"file.txt")<<"collision";
 std::vector<std::wstring> items{(batch/L"file.txt").wstring(),(batch/L"folder").wstring()};
 if(WinBlaze::Actions::MoveItems(items,target.wstring())==0||!std::filesystem::exists(batch/L"folder/child.txt"))return 9;
 std::filesystem::remove(target/L"file.txt");
 if(WinBlaze::Actions::MoveItems(items,target.wstring())!=0||std::filesystem::exists(batch/L"file.txt")||!std::filesystem::exists(target/L"folder/child.txt"))return 10;
 std::vector<std::wstring> recycled{(target/L"file.txt").wstring(),(target/L"folder").wstring()};
 if(WinBlaze::Actions::RecycleItems(recycled)!=0||std::filesystem::exists(target/L"folder"))return 11;
 std::filesystem::remove(target);std::filesystem::remove(batch);
 std::filesystem::remove(existing);std::filesystem::remove(root);std::cout<<"PASS: move without overwrite, file-only recycle, source removed after success\n";
}
