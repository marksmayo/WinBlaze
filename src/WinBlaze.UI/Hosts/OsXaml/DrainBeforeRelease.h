#pragma once
#include <exception>
#include <utility>

// Declare after the owning pointer and framework manager, so this guard runs
// before either is released. Explicit Finish reports cleanup errors; unwinding
// preserves the original exception while still attempting every cleanup step.
template <typename WindowOwner, typename Teardown>
class DrainBeforeRelease
{
public:
    DrainBeforeRelease(WindowOwner& window, Teardown teardown)
        : m_window(window), m_teardown(std::move(teardown)) {}
    DrainBeforeRelease(DrainBeforeRelease const&) = delete;
    DrainBeforeRelease& operator=(DrainBeforeRelease const&) = delete;
    ~DrainBeforeRelease() noexcept { try { Finish(); } catch (...) {} }
    void Finish()
    {
        if (m_finished) return;
        m_finished = true;
        std::exception_ptr failure;
        auto attempt = [&failure](auto&& operation) {
            try { operation(); }
            catch (...) { if (!failure) failure = std::current_exception(); }
        };
        attempt([this] { m_window->Close(); });
        attempt([this] { m_window->DrainBackground(); });
        attempt(m_teardown);
        if (failure) std::rethrow_exception(failure);
    }
private:
    WindowOwner& m_window;
    Teardown m_teardown;
    bool m_finished{};
};
