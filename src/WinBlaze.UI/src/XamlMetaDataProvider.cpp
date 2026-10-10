#include "pch.h"
#include "XamlMetaDataProvider.h"
#include "StartupTrace.h"

#include <winrt/Microsoft.UI.Xaml.XamlTypeInfo.h>

namespace winrt::WinBlaze::UI::implementation
{
    using IXamlType = ::winrt::Microsoft::UI::Xaml::Markup::IXamlType;
    using XmlnsDefinition = ::winrt::Microsoft::UI::Xaml::Markup::XmlnsDefinition;
    using XamlControlsXamlMetaDataProvider = ::winrt::Microsoft::UI::Xaml::XamlTypeInfo::XamlControlsXamlMetaDataProvider;

    IXamlType XamlMetaDataProvider::GetXamlType(::winrt::Windows::UI::Xaml::Interop::TypeName const& type)
    {
        TraceStartup(L"XamlMetaDataProvider::GetXamlType TypeName before framework activation");
        return XamlControlsXamlMetaDataProvider().GetXamlType(type);
    }

    IXamlType XamlMetaDataProvider::GetXamlType(::winrt::hstring const& fullName)
    {
        TraceStartup(L"XamlMetaDataProvider::GetXamlType name before framework activation");
        return XamlControlsXamlMetaDataProvider().GetXamlType(fullName);
    }

    ::winrt::com_array<XmlnsDefinition> XamlMetaDataProvider::GetXmlnsDefinitions()
    {
        TraceStartup(L"XamlMetaDataProvider::GetXmlnsDefinitions before framework activation");
        return XamlControlsXamlMetaDataProvider().GetXmlnsDefinitions();
    }
}
