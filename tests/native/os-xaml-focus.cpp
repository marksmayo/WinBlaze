#include "OsDesktopWindow.h"
#include <winrt/Windows.Foundation.Collections.h>
#include <algorithm>
#include <iostream>
#include <iterator>
#include <stdexcept>

using namespace winrt::Windows::UI::Xaml;
using namespace winrt::Windows::UI::Xaml::Controls;

struct FocusWindow : OsDesktopWindow<FocusWindow>
{
    StackPanel panel;
    Button first;
    Button last;
    FocusWindow()
    {
        first.Content(winrt::box_value(L"First"));
        last.Content(winrt::box_value(L"Last"));
        panel.Children().Append(first);
        panel.Children().Append(last);
        Content(panel);
    }
};

static void Require(bool condition, char const* message)
{
    if (!condition) throw std::runtime_error(message);
}

static void Pump(FocusWindow& window)
{
    const auto until = GetTickCount64() + 250;
    do {
        MSG message{};
        while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
            if (message.message == WM_QUIT) continue;
            if (!window.PreTranslate(message)) {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Sleep(1);
    } while (GetTickCount64() < until);
}

int main()
{
    try {
        winrt::init_apartment(winrt::apartment_type::single_threaded);
        auto manager = winrt::Windows::UI::Xaml::Hosting::WindowsXamlManager::InitializeForCurrentThread();
        auto window = winrt::make_self<FocusWindow>();
        HWND owner{};
        winrt::check_hresult(window.as<IWindowNative>()->get_WindowHandle(&owner));
        DWORD process{};
        const auto thread = GetWindowThreadProcessId(owner, &process);
        Require(process == GetCurrentProcessId() && thread == GetCurrentThreadId(),
            "Focus test must own its window and UI thread");
        window->Activate();
        Pump(*window);
        Require(IsWindowVisible(owner) && IsWindowEnabled(owner),
            "Focus contract requires a visible enabled owned window");
        // Thread-local native focus calls only. No SendInput, global keys or UIA focus bypass.
        SetFocus(nullptr);
        Pump(*window);
        SetFocus(owner);
        std::cout << "owner_focus_before_pump=" << (GetFocus() == owner)
                  << " owner_active=" << (GetActiveWindow() == owner) << '\n';
        Pump(*window);
        const auto focus = GetFocus();
        std::cout << "native_focus_is_child=" << !!IsChild(owner, focus)
                  << " first_focus_state=" << static_cast<int>(window->first.FocusState()) << '\n';
        const bool enters = IsChild(owner, focus) && window->first.FocusState() == FocusState::Keyboard;
        Require(window->last.Focus(FocusState::Keyboard), "Last control must accept keyboard focus");
        std::cout << "last_before_leave=" << static_cast<int>(window->last.FocusState())
                  << " child_before_leave=" << !!IsChild(owner, GetFocus()) << '\n';
        SetFocus(nullptr);
        Pump(*window);
        SetFocus(owner);
        Pump(*window);
        const bool restores = IsChild(owner, GetFocus()) && window->last.FocusState() == FocusState::Keyboard;
        std::cout << "last_control_restored=" << restores
                  << " last_after_return=" << static_cast<int>(window->last.FocusState())
                  << " first_after_return=" << static_cast<int>(window->first.FocusState()) << '\n';
        Require(IsChild(owner, GetFocus()), "Tab message target must belong to this island");
        Require((GetKeyState(VK_SHIFT) & 0x8000) == 0 && (GetKeyState(VK_CONTROL) & 0x8000) == 0 &&
            (GetKeyState(VK_MENU) & 0x8000) == 0, "Tab routing requires no held modifier");
        MSG tab{};
        tab.hwnd = GetFocus();
        tab.message = WM_KEYDOWN;
        tab.wParam = VK_TAB;
        tab.lParam = 1 | (MapVirtualKeyW(VK_TAB, MAPVK_VK_TO_VSC) << 16);
        // Route a message directly through this owned island's documented entry point.
        // This does not inject a key into the desktop or another application's queue.
        window->PreTranslate(tab);
        Pump(*window);
        const bool wraps = IsChild(owner, GetFocus()) && window->first.FocusState() == FocusState::Keyboard;
        std::cout << "last_to_first_wrap=" << wraps << '\n';
        BYTE previous[256]{};
        Require(GetKeyboardState(previous) != FALSE, "Read this thread's keyboard table");
        BYTE reverse[256]{};
        std::copy(std::begin(previous), std::end(previous), std::begin(reverse));
        reverse[VK_SHIFT] |= 0x80;
        reverse[VK_LSHIFT] |= 0x80;
        Require(SetKeyboardState(reverse) != FALSE, "Set this thread's reverse navigation state");
        try { window->PreTranslate(tab); }
        catch (...) { SetKeyboardState(previous); throw; }
        Require(SetKeyboardState(previous) != FALSE, "Restore this thread's keyboard table");
        Pump(*window);
        const bool reverse_wraps = IsChild(owner, GetFocus()) && window->last.FocusState() == FocusState::Keyboard;
        std::cout << "first_to_last_reverse_wrap=" << reverse_wraps << '\n';
        window->Content(StackPanel{});
        Pump(*window);
        SetFocus(nullptr);
        SetFocus(owner);
        Pump(*window);
        const bool empty_returns = IsChild(owner, GetFocus());
        auto weak_window = window->get_weak();
        window->DrainBackground();
        window->Close();
        window = nullptr;
        Require(!weak_window.get(), "Focus event subscription must not retain the closed window");
        manager.Close();
        MSG pending{};
        while (PeekMessageW(&pending, nullptr, 0, 0, PM_REMOVE)) DispatchMessageW(&pending);
        Require(enters, "Native owner focus must enter the first XAML control with keyboard focus");
        Require(restores, "Returning native owner focus must preserve the last focused XAML control");
        Require(wraps, "Forward focus navigation must wrap from the last control to the first");
        Require(reverse_wraps, "Reverse focus navigation must wrap from the first control to the last");
        Require(empty_returns, "An empty island must return from native focus handoff without recursion");
        std::cout << "Owned native-to-XAML focus handoff passed\n";
        return 0;
    } catch (winrt::hresult_error const& error) {
        std::cerr << winrt::to_string(error.message()) << '\n';
        return 2;
    } catch (std::exception const& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
