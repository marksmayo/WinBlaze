#pragma once

#include <unknwn.h>

#undef GetCurrentTime

#include "winrt/Microsoft.UI.Xaml.h"
#include "winrt/Microsoft.UI.Xaml.Markup.h"
#include "winrt/Microsoft.UI.Xaml.Interop.h"
#include "winrt/Windows.UI.Xaml.Interop.h"

#include "XamlMetaDataProvider.h"

namespace winrt::WinBlaze::UI::implementation
{
    template <typename D, typename... Interfaces>
    struct AppT :
        ::winrt::Microsoft::UI::Xaml::ApplicationT<
            D,
            ::winrt::Microsoft::UI::Xaml::Markup::IXamlMetadataProvider,
            Interfaces...>
    {
        using IXamlType = ::winrt::Microsoft::UI::Xaml::Markup::IXamlType;

        void InitializeComponent()
        {
            _contentLoaded = true;
        }

        IXamlType GetXamlType(::winrt::Windows::UI::Xaml::Interop::TypeName const& type)
        {
            return AppProvider()->GetXamlType(type);
        }

        IXamlType GetXamlType(::winrt::hstring const& fullName)
        {
            return AppProvider()->GetXamlType(fullName);
        }

        ::winrt::com_array<::winrt::Microsoft::UI::Xaml::Markup::XmlnsDefinition> GetXmlnsDefinitions()
        {
            return AppProvider()->GetXmlnsDefinitions();
        }

    private:
        bool _contentLoaded{ false };
        winrt::com_ptr<XamlMetaDataProvider> _appProvider;

        winrt::com_ptr<XamlMetaDataProvider> AppProvider()
        {
            if (!_appProvider) {
                _appProvider = winrt::make_self<XamlMetaDataProvider>();
            }
            return _appProvider;
        }
    };
}
