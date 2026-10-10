#pragma once
#include <windows.h>
#include "resource.h"
#include "OwnedBackgroundAwait.h"
#include "OwnedCoroutineLifetime.h"
#ifdef GetCurrentTime
#undef GetCurrentTime
#endif
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Controls.h>
#include <winrt/Windows.UI.Xaml.Hosting.h>
#include <winrt/Windows.UI.Xaml.Input.h>
#include <windows.ui.xaml.hosting.desktopwindowxamlsource.h>
#include <microsoft.ui.xaml.window.h>
#include <thread>
#include <cwchar>
#include <atomic>
#include <memory>
#include "OwnedBackgroundWork.h"

class OsHostDispatcher
{
public:
    OsHostDispatcher(winrt::Windows::System::DispatcherQueue const& queue,
        std::shared_ptr<std::atomic<bool>> closed) : m_queue(queue), m_closed(std::move(closed)) {}
    bool HasThreadAccess() const { return m_queue.HasThreadAccess(); }
    explicit operator bool() const { return static_cast<bool>(m_queue); }
    auto CreateTimer() const { return m_queue.CreateTimer(); }
    bool TryEnqueue(winrt::Windows::System::DispatcherQueueHandler const& handler) const
    {
        if (m_closed->load()) return false;
        return m_queue.TryEnqueue([handler, closed = m_closed] {
            if (!closed->load()) handler();
        });
    }
private:
    winrt::Windows::System::DispatcherQueue m_queue{nullptr};
    std::shared_ptr<std::atomic<bool>> m_closed;
};

inline void OsDesktopRequestExit()
{
    struct ExitResult { DWORD error{}; } result;
    const auto enumerate = [](HWND window, LPARAM parameter) -> BOOL {
        DWORD process{};
        GetWindowThreadProcessId(window, &process);
        if (process != GetCurrentProcessId()) return TRUE;
        wchar_t name[256]{};
        GetClassNameW(window, name, 256);
        if (std::wcscmp(name, L"WinBlazeExperimentalOsXamlHost") != 0) return TRUE;
        // Delivery runs each host's normal close/cleanup on its own UI thread.
        if (!PostMessageW(window, WM_CLOSE, 0, 0)) {
            const auto error = GetLastError();
            if (IsWindow(window)) reinterpret_cast<ExitResult*>(parameter)->error =
                error ? error : ERROR_GEN_FAILURE;
        }
        return TRUE;
    };
    if (!EnumWindows(enumerate, reinterpret_cast<LPARAM>(&result))) winrt::throw_last_error();
    if (result.error) winrt::throw_hresult(HRESULT_FROM_WIN32(result.error));
}

// Alternative desktop-island host. The standard WinUI project does not include this adapter.
template <typename Derived>
struct OsDesktopWindow : winrt::implements<Derived,
    winrt::Windows::Foundation::IStringable, IWindowNative>
{
    using Element = winrt::Windows::UI::Xaml::UIElement;
    using ClosedHandler = winrt::Windows::Foundation::EventHandler<winrt::Windows::Foundation::IInspectable>;

    OsDesktopWindow()
    {
        m_dispatcher = winrt::Windows::System::DispatcherQueue::GetForCurrentThread();
        if (!m_dispatcher) winrt::throw_hresult(E_UNEXPECTED);
        WNDCLASSW cls{};
        cls.lpfnWndProc = Procedure;
        cls.hInstance = GetModuleHandleW(nullptr);
        cls.lpszClassName = L"WinBlazeExperimentalOsXamlHost";
        cls.hCursor = LoadCursorW(nullptr, IDC_ARROW);
        if (!RegisterClassW(&cls) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
            winrt::throw_last_error();
        m_owner = CreateWindowExW(0, cls.lpszClassName, L"WinBlaze", WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT, CW_USEDEFAULT, 1024, 768, nullptr, nullptr, cls.hInstance, this);
        if (!m_owner) winrt::throw_last_error();
        try {
            SetIcons();
            m_source = winrt::Windows::UI::Xaml::Hosting::DesktopWindowXamlSource{};
            m_native = m_source.as<IDesktopWindowXamlSourceNative2>();
            winrt::check_hresult(m_native->AttachToWindow(m_owner));
            winrt::check_hresult(m_native->get_WindowHandle(&m_island));
            Resize();
            m_focus_requested = m_source.TakeFocusRequested(winrt::auto_revoke,
                [this](auto const& source, auto const& args) {
                    using Reason = winrt::Windows::UI::Xaml::Hosting::XamlSourceFocusNavigationReason;
                    const auto reason = args.Request().Reason();
                    if (m_closing || m_navigating_focus ||
                        (reason != Reason::First && reason != Reason::Last)) return;
                    // This window has one island and no native tab stops: cycle within it.
                    m_navigating_focus = true;
                    try { source.NavigateFocus(args.Request()); }
                    catch (...) { m_navigating_focus = false; throw; }
                    m_navigating_focus = false;
                });
        } catch (...) { DestroyWindow(m_owner); m_owner = nullptr; ReleaseIcons(); throw; }
    }
    ~OsDesktopWindow()
    {
        m_dispatch_closed->store(true);
        m_focus_requested.revoke();
        if (m_source) m_source.Close();
        if (m_owner) DestroyWindow(m_owner);
        ReleaseIcons();
    }
    winrt::hstring ToString() const { return L"WinBlaze experimental desktop window"; }
    bool RunBackground(std::function<void()> job) { return m_background.Run(std::move(job)); }
    auto AwaitBackground(std::function<void()> job) {
        return OwnedBackgroundAwait(m_background, std::move(job));
    }
    // Call while the complete derived window is still alive, before releasing
    // its members. The base destructor alone is too late for derived captures.
    auto BeginOwnedCoroutine() { return m_coroutines.Begin(); }
    void DrainBackground() {
        // This drain runs outside XAML callbacks, while the full window lives.
        // Complete a queued close before waiting for work that close cancels.
        CloseNow();
        std::exception_ptr failure;
        try { m_background.CloseAndDrain(); }
        catch (...) { failure = std::current_exception(); }
        m_coroutines.CloseAndDrain([] {
            MSG message{};
            while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
                if (message.message == WM_QUIT) continue;
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        });
        if (failure) std::rethrow_exception(failure);
    }
    bool IsClosing() const { return m_dispatch_closed->load(); }
    winrt::Windows::Foundation::IAsyncOperation<
        winrt::Windows::UI::Xaml::Controls::ContentDialogResult> ShowOwnedDialog(
        winrt::Windows::UI::Xaml::Controls::ContentDialog dialog)
    {
        using Result = winrt::Windows::UI::Xaml::Controls::ContentDialogResult;
        auto lifetime = this->get_strong();
        auto operation = BeginOwnedCoroutine();
        if (!operation || IsClosing()) co_return Result::None;
        m_dialog = dialog;
        Result result = Result::None;
        try { result = co_await dialog.ShowAsync(); }
        catch (...) {
            m_dialog = nullptr;
            if (IsClosing()) co_return Result::None;
            throw;
        }
        m_dialog = nullptr;
        co_return IsClosing() ? Result::None : result;
    }
    HRESULT __stdcall get_WindowHandle(HWND* owner) noexcept
    {
        if (!owner) return E_POINTER;
        *owner = m_owner;
        return m_owner ? S_OK : RO_E_CLOSED;
    }
    Element Content() const { return m_source.Content(); }
    void Content(Element const& element) { m_source.Content(element); }
    void Title(winrt::hstring const& title) { SetWindowTextW(m_owner, title.c_str()); }
    auto DispatcherQueue() const { return OsHostDispatcher(m_dispatcher, m_dispatch_closed); }
    void Closed(ClosedHandler const& handler) { m_closed = handler; }
    void Activate() { ShowWindow(m_owner, SW_SHOW); SetForegroundWindow(m_owner); }
    bool PreTranslate(MSG const& message)
    {
        BOOL handled{};
        winrt::check_hresult(m_native->PreTranslateMessage(&message, &handled));
        return handled != FALSE;
    }
    void Close()
    {
        if (!m_owner || m_closing || m_close_requested) return;
        // A XAML dispatcher/menu callback must return before its island closes.
        // Destroying it inline invalidates the runtime's active callback frame.
        if (!PostMessageW(m_owner, WM_CLOSE, 0, 0)) winrt::throw_last_error();
        m_close_requested = true;
        m_dispatch_closed->store(true);
    }
private:
    void CloseNow()
    {
        if (!m_owner || m_closing) return;
        m_closing = true;
        m_dispatch_closed->store(true);
        m_focus_requested.revoke();
        if (m_dialog) m_dialog.Hide();
        if (m_closed) m_closed(this->get_strong().template as<winrt::Windows::Foundation::IInspectable>(), nullptr);
        m_source.Close();
        m_native = nullptr;
        m_source = nullptr;
        DestroyWindow(m_owner);
        m_owner = nullptr;
        m_island = nullptr;
    }
private:
    HICON m_small_icon{};
    HICON m_large_icon{};
    void ReleaseIcons() noexcept
    {
        if (m_small_icon) { DestroyIcon(m_small_icon); m_small_icon = nullptr; }
        if (m_large_icon) { DestroyIcon(m_large_icon); m_large_icon = nullptr; }
    }
    void SetIcons()
    {
        const auto module = GetModuleHandleW(nullptr);
        m_large_icon = static_cast<HICON>(LoadImageW(module, MAKEINTRESOURCEW(IDI_WINBLAZE),
            IMAGE_ICON, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON), 0));
        if (!m_large_icon) winrt::throw_last_error();
        m_small_icon = static_cast<HICON>(LoadImageW(module, MAKEINTRESOURCEW(IDI_WINBLAZE),
            IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), 0));
        if (!m_small_icon) winrt::throw_last_error();
        SendMessageW(m_owner, WM_SETICON, ICON_BIG, reinterpret_cast<LPARAM>(m_large_icon));
        SendMessageW(m_owner, WM_SETICON, ICON_SMALL, reinterpret_cast<LPARAM>(m_small_icon));
    }
    winrt::Windows::UI::Xaml::Controls::ContentDialog m_dialog{nullptr};
    void Resize()
    {
        if (!m_island) return;
        RECT bounds{};
        GetClientRect(m_owner, &bounds);
        SetWindowPos(m_island, nullptr, 0, 0, bounds.right, bounds.bottom, SWP_NOZORDER | SWP_SHOWWINDOW);
    }
    static LRESULT CALLBACK Procedure(HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam)
    {
        auto self = reinterpret_cast<OsDesktopWindow*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
        if (message == WM_NCCREATE) {
            self = static_cast<OsDesktopWindow*>(reinterpret_cast<CREATESTRUCTW*>(lparam)->lpCreateParams);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        }
        if (self) {
            if (message == WM_SETFOCUS && self->m_source && !self->m_closing) {
                // A native owner gaining focus does not automatically enter its XAML island.
                // Restore preserves the last focused control after switching windows.
                using namespace winrt::Windows::UI::Xaml::Hosting;
                SetFocus(self->m_island);
                const auto result = self->m_source.NavigateFocus(XamlSourceFocusNavigationRequest(
                    XamlSourceFocusNavigationReason::Restore));
                if (!result.WasFocusMoved()) {
                    self->m_source.NavigateFocus(XamlSourceFocusNavigationRequest(
                        XamlSourceFocusNavigationReason::First));
                }
                // Restoring the native island can retain pointer focus state.
                // Preserve the chosen control and establish keyboard focus for
                // this explicit native-to-XAML navigation handoff.
                if (auto content = self->m_source.Content()) {
                    if (auto root = content.XamlRoot()) {
                        if (auto control = winrt::Windows::UI::Xaml::Input::FocusManager::
                            GetFocusedElement(root).try_as<winrt::Windows::UI::Xaml::Controls::Control>()) {
                            control.Focus(winrt::Windows::UI::Xaml::FocusState::Keyboard);
                        }
                    }
                }
                return 0;
            }
            if (message == WM_SIZE) self->Resize();
            if (message == WM_CLOSE) { self->CloseNow(); return 0; }
            if (message == WM_DPICHANGED) {
                auto bounds = reinterpret_cast<RECT*>(lparam);
                SetWindowPos(hwnd, nullptr, bounds->left, bounds->top,
                    bounds->right - bounds->left, bounds->bottom - bounds->top, SWP_NOZORDER | SWP_NOACTIVATE);
                return 0;
            }
        }
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    HWND m_owner{};
    HWND m_island{};
    bool m_closing{};
    bool m_close_requested{};
    bool m_navigating_focus{};
    winrt::Windows::System::DispatcherQueue m_dispatcher{nullptr};
    winrt::Windows::UI::Xaml::Hosting::DesktopWindowXamlSource m_source{nullptr};
    winrt::Windows::UI::Xaml::Hosting::DesktopWindowXamlSource::TakeFocusRequested_revoker m_focus_requested;
    winrt::com_ptr<IDesktopWindowXamlSourceNative2> m_native;
    ClosedHandler m_closed{nullptr};
    std::shared_ptr<std::atomic<bool>> m_dispatch_closed = std::make_shared<std::atomic<bool>>(false);
    OwnedBackgroundWork m_background;
    OwnedCoroutineLifetime m_coroutines;
};
