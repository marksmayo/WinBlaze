#include "../../src/WinBlaze.UI/src/ByteFormat.h"

#include <cstdlib>
#include <iostream>
#include <limits>
#include <new>
#include <sstream>
#include <vector>

namespace
{
    bool counting = false;
    size_t allocated_bytes = 0;
    size_t allocations = 0;

    std::wstring Reference(uint64_t size)
    {
        const double value = static_cast<double>(size);
        std::wostringstream stream;
        stream.setf(std::ios::fixed);
        stream.precision(1);
        if (value >= 1073741824.0) stream << value / 1073741824.0 << L" GB";
        else if (value >= 1048576.0) stream << value / 1048576.0 << L" MB";
        else if (value >= 1024.0) stream << value / 1024.0 << L" KB";
        else { stream.precision(0); stream << value << L" B"; }
        return stream.str();
    }
}

void* operator new(size_t size)
{
    if (auto result = std::malloc(size ? size : 1)) {
        if (counting) { allocated_bytes += size; ++allocations; }
        return result;
    }
    throw std::bad_alloc();
}
void* operator new[](size_t size) { return ::operator new(size); }
void operator delete(void* pointer) noexcept { std::free(pointer); }
void operator delete[](void* pointer) noexcept { std::free(pointer); }
void operator delete(void* pointer, size_t) noexcept { std::free(pointer); }
void operator delete[](void* pointer, size_t) noexcept { std::free(pointer); }

int main()
{
    using WinBlaze::UI::FormatByteSize;
    if (FormatByteSize(0) != L"0 B" || FormatByteSize(1023) != L"1023 B" ||
        FormatByteSize(1024) != L"1.0 KB" || FormatByteSize(1048576) != L"1.0 MB" ||
        FormatByteSize(1073741824) != L"1.0 GB") return 1;

    std::vector<uint64_t> values;
    for (uint64_t value = 0; value <= 1024; ++value) values.push_back(value);
    for (auto threshold : {1024ULL, 1048576ULL, 1073741824ULL}) {
        for (uint64_t offset = 0; offset <= 1024; ++offset) {
            values.push_back(threshold - offset);
            values.push_back(threshold + offset);
        }
    }
    values.push_back(std::numeric_limits<uint64_t>::max());
    uint64_t random = 17;
    for (int index = 0; index < 16384; ++index) {
        random = random * 6364136223846793005ULL + 1442695040888963407ULL;
        values.push_back(random);
    }
    for (auto value : values) {
        if (FormatByteSize(value) != Reference(value)) {
            std::cerr << "Byte text changed for " << value << '\n';
            return 2;
        }
    }

    // A UI label needs at most its returned text allocation, not a stream buffer.
    counting = true;
    size_t characters = 0;
    for (auto value : values) characters += FormatByteSize(value).size();
    counting = false;
    std::cout << "Formatted " << values.size() << " labels, " << characters
        << " characters; allocated " << allocated_bytes << " bytes in "
        << allocations << " allocations\n";
    if (allocations > values.size() || allocated_bytes > values.size() * 64) return 3;
    std::cout << "PASS: byte labels preserve thresholds, rounding and full uint64 range within the allocation budget\n";
}
