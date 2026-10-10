#pragma once
#include <windows.h>
#include <array>
#include <string>
#include <stdexcept>
#include <cwctype>
#include <deque>
#include <vector>
#include <utility>

namespace WinBlaze {
struct ChangeEvent {
    DWORD action{};
    uint64_t observed_utc{};
    std::wstring path;
};
class DirectoryWatcher {
    std::deque<ChangeEvent> m_history;
    uint64_t m_dropped{};
    void Record(DWORD action, std::wstring path) {
        FILETIME now{};
        GetSystemTimePreciseAsFileTime(&now);
        const uint64_t observed = (static_cast<uint64_t>(now.dwHighDateTime) << 32) | now.dwLowDateTime;
        if (m_history.size() == 1000) { m_history.pop_front(); ++m_dropped; }
        m_history.push_back({action, observed, std::move(path)});
    }
    HANDLE m_handle{INVALID_HANDLE_VALUE};
    OVERLAPPED m_pending{};
    alignas(DWORD) std::array<BYTE,65536> m_buffer{};
    std::wstring m_root,m_excluded,m_display_root;
    static std::wstring Lower(std::wstring path){for(auto& ch:path)ch=static_cast<wchar_t>(towlower(ch));while(!path.empty()&&path.back()==L'\\')path.pop_back();return path;}
    void Arm(){
        ResetEvent(m_pending.hEvent);
        if(!ReadDirectoryChangesW(m_handle,m_buffer.data(),static_cast<DWORD>(m_buffer.size()),TRUE,
            FILE_NOTIFY_CHANGE_FILE_NAME|FILE_NOTIFY_CHANGE_DIR_NAME|FILE_NOTIFY_CHANGE_SIZE|FILE_NOTIFY_CHANGE_LAST_WRITE,nullptr,&m_pending,nullptr))throw std::runtime_error("Directory monitoring could not start");
    }
public:
    DirectoryWatcher(std::wstring const& root,std::wstring const& excluded):m_root(Lower(root)),m_excluded(Lower(excluded)),m_display_root(root){
        m_handle=CreateFileW(root.c_str(),FILE_LIST_DIRECTORY,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OVERLAPPED,nullptr);
        if(m_handle==INVALID_HANDLE_VALUE)throw std::runtime_error("Directory monitoring requires an accessible folder");
        m_pending.hEvent=CreateEventW(nullptr,TRUE,FALSE,nullptr);
        try{if(!m_pending.hEvent)throw std::runtime_error("Directory monitoring event could not start");Arm();}
        catch(...){if(m_pending.hEvent)CloseHandle(m_pending.hEvent);CloseHandle(m_handle);throw;}
    }
    DirectoryWatcher(DirectoryWatcher const&)=delete;
    DirectoryWatcher& operator=(DirectoryWatcher const&)=delete;
    ~DirectoryWatcher(){CancelIoEx(m_handle,&m_pending);DWORD bytes{};GetOverlappedResult(m_handle,&m_pending,&bytes,TRUE);CloseHandle(m_pending.hEvent);CloseHandle(m_handle);}
    std::vector<ChangeEvent> TakeEvents() {
        std::vector<ChangeEvent> events;
        events.reserve(m_history.size());
        while (!m_history.empty()) { events.push_back(std::move(m_history.front())); m_history.pop_front(); }
        return events;
    }
    uint64_t DroppedEvents() const { return m_dropped; }
    bool Poll(){
        DWORD bytes{};
        if(!GetOverlappedResult(m_handle,&m_pending,&bytes,FALSE)){
            if(GetLastError()==ERROR_IO_INCOMPLETE)return false;
            throw std::runtime_error("Directory monitoring stopped; enable it again to retry");
        }
        if(bytes==0)Record(0,m_display_root); // Explicit overflow history marker.
        bool changed=bytes==0; // Overflow means rescan, never silently lose updates.
        for(DWORD offset=0;offset<bytes;){
            if(bytes-offset<12){changed=true;break;}
            auto record=reinterpret_cast<FILE_NOTIFY_INFORMATION const*>(m_buffer.data()+offset);
            if(record->FileNameLength%2||record->FileNameLength>bytes-offset-12){changed=true;break;}
            auto path=Lower(m_root+L"\\"+std::wstring(record->FileName,record->FileNameLength/sizeof(wchar_t)));
            const bool excluded=!m_excluded.empty()&&(path==m_excluded||(path.size()>m_excluded.size()&&path.compare(0,m_excluded.size(),m_excluded)==0&&path[m_excluded.size()]==L'\\'));
            changed|=!excluded;
            if(!excluded)Record(record->Action,m_display_root+L"\\"+std::wstring(record->FileName,record->FileNameLength/sizeof(wchar_t)));
            if(!record->NextEntryOffset)break;
            if(record->NextEntryOffset<12||record->NextEntryOffset%4||record->NextEntryOffset>bytes-offset){changed=true;break;}
            offset+=record->NextEntryOffset;
        }
        Arm();return changed;
    }
};
}
