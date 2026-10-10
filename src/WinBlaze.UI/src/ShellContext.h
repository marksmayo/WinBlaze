#pragma once
#include <windows.h>
#include <shlobj.h>
#include <commctrl.h>
#include <wrl/client.h>
#include <algorithm>
#include <string>
#include <vector>
#pragma comment(lib,"shell32.lib")
#pragma comment(lib,"ole32.lib")
#pragma comment(lib,"comctl32.lib")

namespace WinBlaze::Shell {
class Context {
    Microsoft::WRL::ComPtr<IContextMenu> context;
    Microsoft::WRL::ComPtr<IContextMenu2> messages;
    Microsoft::WRL::ComPtr<IContextMenu3> extended_messages;
    PIDLIST_ABSOLUTE absolute{};
    HMENU popup{};
    HWND owner{};
    static LRESULT CALLBACK Relay(HWND window,UINT message,WPARAM wp,LPARAM lp,
        UINT_PTR id,DWORD_PTR data) {
        auto self=reinterpret_cast<Context*>(data);
        if(message==WM_INITMENUPOPUP||message==WM_DRAWITEM||message==WM_MEASUREITEM||message==WM_MENUCHAR){
            LRESULT result{};
            if(self->extended_messages&&SUCCEEDED(self->extended_messages->HandleMenuMsg2(message,wp,lp,&result)))return result;
            if(self->messages&&SUCCEEDED(self->messages->HandleMenuMsg(message,wp,lp)))return 0;
        }
        (void)id;
        return DefSubclassProc(window,message,wp,lp);
    }
    void Collect(HMENU menu,std::vector<std::string>& verbs) const {
        for(int index=0;index<GetMenuItemCount(menu);++index){
            if(auto child=GetSubMenu(menu,index))Collect(child,verbs);
            auto id=GetMenuItemID(menu,index);
            if(id>=1&&id<0x7fff){
                char name[256]{};
                if(SUCCEEDED(context->GetCommandString(id-1,GCS_VERBA,nullptr,name,sizeof(name)))&&name[0])
                    verbs.emplace_back(name);
            }
        }
    }
public:
    Context()=default;
    Context(Context const&)=delete;
    Context& operator=(Context const&)=delete;
    ~Context(){if(popup)DestroyMenu(popup);if(absolute)CoTaskMemFree(absolute);}
    HRESULT Build(std::wstring const& path,HWND window) {
        if(popup||absolute||path.empty()||path.find(L'\0')!=std::wstring::npos)return E_INVALIDARG;
        if(GetFileAttributesW(path.c_str())==INVALID_FILE_ATTRIBUTES)return HRESULT_FROM_WIN32(GetLastError());
        owner=window;
        auto result=SHParseDisplayName(path.c_str(),nullptr,&absolute,0,nullptr);
        if(FAILED(result))return result;
        Microsoft::WRL::ComPtr<IShellFolder> parent;
        PCUITEMID_CHILD child{};
        result=SHBindToParent(absolute,IID_PPV_ARGS(&parent),&child);
        if(FAILED(result))return result;
        result=parent->GetUIObjectOf(owner,1,&child,IID_IContextMenu,nullptr,reinterpret_cast<void**>(context.GetAddressOf()));
        if(FAILED(result))return result;
        popup=CreatePopupMenu();
        if(!popup)return HRESULT_FROM_WIN32(GetLastError());
        result=context->QueryContextMenu(popup,0,1,0x7fff,CMF_NORMAL | CMF_CANRENAME);
        if(FAILED(result))return result;
        context.As(&messages);context.As(&extended_messages);
        return S_OK;
    }
    std::vector<std::string> Verbs() const {
        std::vector<std::string> verbs;
        if(context&&popup)Collect(popup,verbs);
        return verbs;
    }
    HRESULT Show(POINT point) {
        if(!context||!popup||!owner)return E_INVALIDARG;
        constexpr UINT_PTR subclass_id=0x5742434d;
        if(!SetWindowSubclass(owner,&Relay,subclass_id,reinterpret_cast<DWORD_PTR>(this)))
            return HRESULT_FROM_WIN32(GetLastError());
        SetForegroundWindow(owner);
        auto command=TrackPopupMenuEx(popup,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,owner,nullptr);
        RemoveWindowSubclass(owner,&Relay,subclass_id);
        PostMessageW(owner,WM_NULL,0,0);
        if(!command)return S_FALSE;
        CMINVOKECOMMANDINFOEX invoke{};
        invoke.cbSize=sizeof(invoke);
        invoke.fMask=CMIC_MASK_UNICODE|CMIC_MASK_PTINVOKE;
        invoke.hwnd=owner;
        invoke.lpVerb=MAKEINTRESOURCEA(command-1);
        invoke.lpVerbW=MAKEINTRESOURCEW(command-1);
        invoke.nShow=SW_SHOWNORMAL;invoke.ptInvoke=point;
        return context->InvokeCommand(reinterpret_cast<CMINVOKECOMMANDINFO*>(&invoke));
    }
};
}
