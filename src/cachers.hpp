#pragma once

#include <coroutine>
#include <cstddef>
#include <expected>
#include <optional>
#include <span>
#include <string_view>
#include <utility>

#include "cachers.h"

namespace cachers
{

template <typename T>
using ptr = T*;

using error_code = cachers_err;

template <typename T, typename E = error_code>
using expected = std::expected<T, E>;

using std::unexpected;

using key_view = std::span<std::byte const>;
using header_view = std::span<std::byte const>;
using data_view = std::span<std::byte const>;

namespace detail
{

template <typename T, auto FDestroyHandle>
class unique_handle
{
public:
    constexpr unique_handle() noexcept
        : _handle{nullptr}
    { }

    constexpr unique_handle(ptr<T> p) noexcept
        : _handle{p}
    { }

    unique_handle(unique_handle const&) = delete;
    unique_handle& operator=(unique_handle const&) = delete;

    unique_handle(unique_handle&& src) noexcept
        : _handle{std::exchange(src._handle, nullptr)}
    { }

    unique_handle& operator=(unique_handle&& src) noexcept
    {
        reset();
        _handle = std::exchange(src._handle, nullptr);
        return *this;
    }

    constexpr ptr<T> get() const noexcept
    {
        return _handle;
    }

    ptr<T> release()
    {
        return std::exchange(_handle, nullptr);
    }

    void reset()
    {
        if (auto handle = release())
            FDestroyHandle(handle);
    }

private:
    ptr<T> _handle;
};

}

class response
{
private:
    friend class database;

    explicit response(cachers_response&& resp)
        : _impl{resp}
        , _token{resp.token}
    { }

    header_view header() const
    {
        return { reinterpret_cast<ptr<std::byte const>>(_impl.header), _impl.header_size };
    }

    friend auto operator co_await(response self)
    {
        struct awaiter
        {
            response _target;
            std::optional<std::coroutine_handle<>> _waker;

            explicit awaiter(response&& src) noexcept
                : _target{std::move(src)}
                , _waker{std::nullopt}
            { }

            bool await_ready() const
            {
                return _target._impl.data_state != CACHERS_STATE_IN_PROGRESS;
            }

            void await_suspend(std::coroutine_handle<> waker)
            {
                _waker.emplace(std::move(waker));
                auto rc = cachers_response_get_or_bind(
                    _target._token.get(),
                    [](ptr<const cachers_response> delivered, ptr<void> pawaiter_v)
                    {
                        auto pawaiter = reinterpret_cast<ptr<awaiter>>(pawaiter_v);
                        pawaiter->_target._impl = *delivered;
                        pawaiter->_waker->resume();
                    },
                    this,
                    &_target._impl
                );

                if (rc == CACHERS_ERR_HAS_DATA)
                {
                    // if we got the data, resume immediately
                    this->_waker->resume();
                }
            }

            expected<response> await_resume()
            {
                return std::move(_target);
            }
        };
        return awaiter{std::move(self)};
    }

private:
    cachers_response _impl;
    detail::unique_handle<cachers_response_token, cachers_response_token_release> _token;
};

class database
{
public:
    static expected<database> open()
    {
        ptr<cachers_db> p{};
        if (auto err = cachers_open(&p))
            return unexpected{err};
        return database{p};
    }

    expected<response> get(key_view key) const
    {
        cachers_response resp{};
        if (auto err = cachers_get(_impl.get(), reinterpret_cast<ptr<void const>>(key.data()), key.size(), &resp))
            return unexpected{err};
        return response{std::move(resp)};
    }

    expected<response> get(std::string_view key) const
    {
        return get(key_view{ reinterpret_cast<ptr<std::byte const>>(key.data()), key.size() });
    }

private:
    explicit database(ptr<cachers_db> impl) noexcept
        : _impl{impl}
    { }

private:
    detail::unique_handle<cachers_db, cachers_release> _impl;
};

}
