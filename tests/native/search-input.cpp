#include "../../src/WinBlaze.UI/src/SearchInput.h"
#include <stdexcept>
#include <iostream>
int main() {
    using WinBlaze::UI::ParseByteSize;
    if (ParseByteSize(L"4096") != 4096 || ParseByteSize(L" 4 KiB ") != 4096 ||
        ParseByteSize(L"0.5 MB") != 524288 || ParseByteSize(L"0") != 0)
        throw std::runtime_error("Valid byte bounds rejected");
    for (auto value : {L"", L"-1", L"nan", L"1 frogs", L"1 KB extra", L"18446744073709551616"})
        if (ParseByteSize(value)) throw std::runtime_error("Invalid size accepted");
    std::cout << "PASS: byte bounds accept bare values and units, reject malformed input and overflow\n";
}
