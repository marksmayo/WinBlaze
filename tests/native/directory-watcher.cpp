#include "../../src/WinBlaze.UI/src/DirectoryWatcher.h"
#include <filesystem>
#include <fstream>
#include <iostream>
int wmain(int argc,wchar_t** argv){
 if(argc!=2)return 2;std::filesystem::path root(argv[1]);std::filesystem::create_directories(root/L"ignored");
 WinBlaze::DirectoryWatcher watcher(root.wstring(),(root/L"ignored").wstring());
 std::ofstream(root/L"ignored"/L"internal")<<"ignore";Sleep(100);
 if(watcher.Poll() || !watcher.TakeEvents().empty())return 1;
 auto changed=[&](){for(int i=0;i<100;++i){if(watcher.Poll())return true;Sleep(10);}return false;};
 std::ofstream(root/L"external")<<"add";if(!changed())return 3;
 std::ofstream(root/L"external",std::ios::app)<<"modify";if(!changed())return 4;
 std::filesystem::remove(root/L"external");if(!changed())return 5;
 auto events=watcher.TakeEvents();
 bool added=false,modified=false,removed=false;
 for(auto const& event:events){
   if(!event.observed_utc || event.path!=(root/L"external").wstring())return 6;
   added|=event.action==FILE_ACTION_ADDED;modified|=event.action==FILE_ACTION_MODIFIED;removed|=event.action==FILE_ACTION_REMOVED;
 }
 if(!added||!modified||!removed)return 7;
 std::ofstream(root/L"before")<<"rename";if(!changed())return 8;watcher.TakeEvents();
 std::filesystem::rename(root/L"before",root/L"after");if(!changed())return 9;
 bool oldName=false,newName=false;
 for(int wait=0;wait<100&&(!oldName||!newName);++wait){
   watcher.Poll();events=watcher.TakeEvents();
   for(auto const& event:events){oldName|=event.action==FILE_ACTION_RENAMED_OLD_NAME;newName|=event.action==FILE_ACTION_RENAMED_NEW_NAME;}
   Sleep(10);
 }
 if(!oldName||!newName)return 10;
 std::filesystem::remove(root/L"after");
 std::cout<<"PASS: add/modify/remove/rename history with observation times, ignored internal writes\n";
}
