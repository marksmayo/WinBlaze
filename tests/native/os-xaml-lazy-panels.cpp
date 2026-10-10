#include "full-os-candidate/MainWindow.xaml.h"
#include <iostream>

using namespace winrt::Windows::UI::Xaml;
using namespace winrt::Windows::UI::Xaml::Controls;

static FrameworkElement Find(winrt::Windows::Foundation::IInspectable const& value, std::wstring_view name)
{
    if (!value) return nullptr;
    auto element = value.try_as<FrameworkElement>();
    if (element && Automation::AutomationProperties::GetName(element) == name) return element;
    if (auto text = value.try_as<TextBlock>(); text && text.Text() == name) return text;
    auto inspect = [&](auto const& child) { return Find(child, name); };
    if (auto panel = value.try_as<Panel>()) {
        for (auto child : panel.Children()) if (auto found = inspect(child)) return found;
    } else if (auto border = value.try_as<Border>()) {
        if (auto found = inspect(border.Child())) return found;
    } else if (auto scroll = value.try_as<ScrollViewer>()) {
        if (auto found = inspect(scroll.Content())) return found;
    } else if (auto content = value.try_as<ContentControl>()) {
        if (auto found = inspect(content.Content())) return found;
    }
    return nullptr;
}

static void Require(bool value, char const* message) { if (!value) throw std::runtime_error(message); }

int main()
{
    try {
        winrt::init_apartment(winrt::apartment_type::single_threaded);
        auto manager = Hosting::WindowsXamlManager::InitializeForCurrentThread();
        auto window = winrt::make_self<winrt::WinBlaze::UI::implementation::MainWindow>();
        auto root = window->Content();
        Require(bool(Find(root, L"Start scan")), "Visible scan controls must be ready immediately");
        Require(!Find(root, L"Search query"), "Initially hidden search must not allocate its control subtree");
        Require(!Find(root, L"Runtime metrics"), "Initially hidden diagnostics must not allocate its control subtree");
        Require(!Find(root, L"Details"), "Initially hidden details must not allocate its control subtree");
        using Section = winrt::WinBlaze::UI::implementation::ShellSection;
        window->SetSection(Section::Search);
        auto query = Find(root, L"Search query").as<TextBox>();
        query.Text(L"preserved query");
        window->SetSection(Section::Overview);
        window->SetSection(Section::Search);
        Require(Find(root, L"Search query") == query && query.Text() == L"preserved query", "Reopening search must preserve its controls and filters");
        window->SetSection(Section::Diagnostics);
        Require(bool(Find(root, L"Runtime metrics")), "Diagnostics must be available when requested");
        auto detail = Find(root, L"Folder view").as<CheckBox>();
        detail.IsChecked(true);
        window->OnOptionalPanelToggleClicked(nullptr, nullptr);
        Require(bool(Find(root, L"Details")), "Details must be available through the existing view toggle");
        window->Close();
        window = nullptr;
        manager.Close();
        std::cout << "PASS: visible shell ready, hidden panels deferred, reveal and filter state preserved\n";
    } catch (std::exception const& error) { std::cerr << error.what() << '\n'; return 1; }
      catch (winrt::hresult_error const& error) { std::wcerr << error.message().c_str() << '\n'; return 2; }
}
