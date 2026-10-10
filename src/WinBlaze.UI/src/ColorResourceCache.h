#pragma once
#include <array>
#include <cstdint>
#include <optional>
#include <utility>
namespace WinBlaze {
// UI-thread-owned palette cache. Callers treat returned color resources as immutable.
// Rare colors beyond Capacity are constructed without retaining another object.
template<class Resource, size_t Capacity = 32>
class ColorResourceCache {
    struct Entry { uint32_t color; Resource resource; };
    std::array<std::optional<Entry>, Capacity> m_entries{};
    size_t m_size{};
public:
    template<class Factory>
    Resource Get(uint32_t color, Factory&& factory) {
        for(size_t i=0;i<m_size;++i)if(m_entries[i]->color==color)return m_entries[i]->resource;
        auto resource=std::forward<Factory>(factory)(color);
        if(m_size<Capacity)m_entries[m_size++].emplace(Entry{color,resource});
        return resource;
    }
};
}
