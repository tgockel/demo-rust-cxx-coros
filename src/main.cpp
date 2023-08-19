#include <future>
#include <thread>

#include "cachers.hpp"

template <typename T>
struct std::coroutine_traits<std::future<T>>
{
    struct promise_type : std::promise<T>
    {
        std::future<T> get_return_object()
        {
            return this->get_future();
        }

        std::suspend_never initial_suspend() noexcept
        {
            return {};
        }

        std::suspend_never final_suspend() noexcept
        {
            return {};
        }

        template <typename U>
        void return_value(U&& value)
        {
            this->set_value(std::forward<U>(value));
        }

        void unhandled_exception()
        {
            this->set_exception(std::current_exception());
        }
    };
};

template <typename T>
auto operator co_await(std::future<T> src)
{
    struct awaiter
    {
        std::future<T> impl;

        bool await_ready()
        {
            return false;
        }

        void await_suspend(std::coroutine_handle<> handle)
        {
            std::thread([this, handle]()
            {
                this->impl.wait();
                handle.resume();
            }).detach();
        }

        T await_resume()
        {
            return this->impl.get();
        }
    };
    return awaiter{std::move(src)};
}

std::future<cachers::response> run()
{
    auto db = cachers::database::open().value();
    auto resp = co_await db.get("test").value();

    co_return std::move(resp.value());
}

int main()
{
    run();
}
