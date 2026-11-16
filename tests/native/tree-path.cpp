#include "../../src/WinBlaze.UI/src/TreePath.h"

#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <new>
#include <string>
#include <vector>

namespace
{
    bool counting = false;
    size_t allocations = 0;

    struct Node
    {
        std::wstring name;
        size_t parent{ SIZE_MAX };
    };

    // MainWindow::TreeNodePath as it was before the fast path, kept as the oracle.
    std::wstring Reference(std::vector<Node> const& nodes, size_t node_index)
    {
        std::vector<size_t> chain;
        size_t current = node_index;
        while (current != SIZE_MAX && current < nodes.size()) {
            chain.push_back(current);
            current = nodes[current].parent;
        }
        std::wstring path;
        for (auto it = chain.rbegin(); it != chain.rend(); ++it) {
            auto const& segment = nodes[*it].name;
            if (path.empty() || std::filesystem::path(segment).is_absolute()) {
                path = segment;
            } else {
                if (!path.empty() && path.back() != L'\\') {
                    path += L'\\';
                }
                path += segment;
            }
        }
        return path;
    }

    int failures = 0;
    void Expect(bool condition, char const* message)
    {
        if (!condition) {
            std::cerr << "FAIL: " << message << '\n';
            ++failures;
        }
    }

    void ExpectSameForEveryNode(std::vector<Node> const& nodes, char const* label)
    {
        for (size_t index = 0; index <= nodes.size() + 1; ++index) {
            if (WinBlaze::UI::TreePath::Build(nodes, index) != Reference(nodes, index)) {
                std::cerr << "FAIL: " << label << " differs at node " << index << '\n';
                ++failures;
                return;
            }
        }
        if (WinBlaze::UI::TreePath::Build(nodes, SIZE_MAX) != Reference(nodes, SIZE_MAX)) {
            std::cerr << "FAIL: " << label << " differs for SIZE_MAX\n";
            ++failures;
        }
    }
}

void* operator new(size_t size)
{
    if (auto result = std::malloc(size ? size : 1)) {
        if (counting) { ++allocations; }
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
    using WinBlaze::UI::TreePath::Build;
    using WinBlaze::UI::TreePath::MayBeAbsolute;

    // A segment without a separator can never be absolute on Windows.
    for (auto const* segment : { L"", L"name", L"C:", L"C:foo", L"file.txt", L"日本", L"..", L"." }) {
        Expect(!MayBeAbsolute(segment), "separator-free segment flagged");
        Expect(!std::filesystem::path(segment).is_absolute(), "oracle disagrees on separator-free segment");
    }
    for (auto const* segment : { L"C:\\", L"C:/", L"\\\\?\\C:\\", L"\\\\server\\share\\", L"\\foo", L"a\\b" }) {
        Expect(MayBeAbsolute(segment), "segment with separator not checked");
    }

    // Root with and without a trailing separator, nested mounts, UNC and
    // long-path prefixes, rooted-but-relative segments, forward slashes,
    // empty names, and names longer than the small-string buffer.
    const std::vector<Node> shapes{
        { L"C:\\", SIZE_MAX },                         // 0
        { L"Users", 0 },                               // 1
        { L"a much longer directory name than SSO", 1 }, // 2
        { L"file.txt", 2 },                            // 3
        { L"", 2 },                                    // 4
        { L"D:\\mounted", 2 },                         // 5 absolute: restarts
        { L"inner", 5 },                               // 6
        { L"\\rooted", 1 },                            // 7 rooted, not absolute
        { L"C:relative", 1 },                          // 8 drive-relative
        { L"C:/forward", 1 },                          // 9 absolute with '/'
        { L"after", 9 },                               // 10
        { L"\\\\?\\E:\\long", 1 },                     // 11 long-path prefix
        { L"x", 11 },                                  // 12
        { L"\\\\server\\share\\", 1 },                 // 13 UNC
        { L"y", 13 },                                  // 14
        { L"trailing\\", 1 },                          // 15
        { L"z", 15 },                                  // 16
        { L"C:", SIZE_MAX },                           // 17 root without separator
        { L"child", 17 },                              // 18
        { L"orphan", 99 },                             // 19 parent out of range
        { L"日本語のフォルダ", 2 },                      // 20
    };
    ExpectSameForEveryNode(shapes, "shape tree");
    Expect(Build(shapes, 3) == L"C:\\Users\\a much longer directory name than SSO\\file.txt", "plain path");
    Expect(Build(shapes, 6) == L"D:\\mounted\\inner", "absolute segment restarts the path");
    Expect(Build(shapes, 18) == L"C:\\child", "separator added after bare drive");
    Expect(Build(shapes, 999).empty(), "out-of-range node is empty");

    // Seeded random trees over a pool of awkward segments.
    const wchar_t* pool[] = {
        L"a", L"B", L"", L"C:\\", L"C:", L"\\x", L"y\\", L"D:/q", L"\\\\?\\F:\\", L"\\\\h\\s\\",
        L"long segment name exceeding the buffer", L"é", L"..", L"/", L"\\", L"G:\\deep\\er",
    };
    uint64_t state = 0x9E3779B97F4A7C15ULL;
    auto next = [&](uint64_t bound) {
        state ^= state << 13; state ^= state >> 7; state ^= state << 17;
        return state % bound;
    };
    for (int round = 0; round < 300; ++round) {
        std::vector<Node> nodes;
        const size_t count = 1 + next(40);
        for (size_t index = 0; index < count; ++index) {
            const size_t parent = index == 0 || next(10) == 0
                ? (next(2) ? SIZE_MAX : count + next(5))
                : next(index);
            nodes.push_back({ pool[next(std::size(pool))], parent });
        }
        ExpectSameForEveryNode(nodes, "random tree");
    }

    // Allocation budget: one chain buffer and one exactly-sized result.
    std::vector<Node> deep{ { L"C:\\", SIZE_MAX } };
    for (size_t depth = 1; depth <= 12; ++depth) {
        deep.push_back({ L"segment-name-longer-than-small-string-" + std::to_wstring(depth), depth - 1 });
    }
    const size_t leaf = deep.size() - 1;
    counting = true;
    allocations = 0;
    const auto reference_path = Reference(deep, leaf);
    const size_t reference_allocations = allocations;
    allocations = 0;
    const auto fast_path = Build(deep, leaf);
    const size_t fast_allocations = allocations;
    counting = false;
    Expect(fast_path == reference_path, "deep path differs");
    std::cout << "Deep path: reference " << reference_allocations << " allocations, fast "
              << fast_allocations << " allocations\n";
    Expect(fast_allocations <= 2, "fast path exceeds two allocations");

    if (failures != 0) {
        return 1;
    }
    std::cout << "PASS: tree paths match the filesystem::path oracle within the allocation budget\n";
    return 0;
}
