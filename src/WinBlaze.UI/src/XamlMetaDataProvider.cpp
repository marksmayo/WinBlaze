#include "pch.h"
#include "XamlMetaDataProvider.h"

#include <winrt/Microsoft.UI.Xaml.XamlTypeInfo.h>

namespace winrt::WinBlaze::UI::implementation
{
    using IXamlType = ::winrt::Microsoft::UI::Xaml::Markup::IXamlType;
    using XmlnsDefinition = ::winrt::Microsoft::UI::Xaml::Markup::XmlnsDefinition;
    using XamlControlsXamlMetaDataProvider = ::winrt::Microsoft::UI::Xaml::XamlTypeInfo::XamlControlsXamlMetaDataProvider;

    IXamlType XamlMetaDataProvider::GetXamlType(::winrt::Windows::UI::Xaml::Interop::TypeName const& type)
    {
        return XamlControlsXamlMetaDataProvider().GetXamlType(type);
    }

    IXamlType XamlMetaDataProvider::GetXamlType(::winrt::hstring const& fullName)
    {
        return XamlControlsXamlMetaDataProvider().GetXamlType(fullName);
    }

    ::winrt::com_array<XmlnsDefinition> XamlMetaDataProvider::GetXmlnsDefinitions()
    {
        return XamlControlsXamlMetaDataProvider().GetXmlnsDefinitions();
    }
}
