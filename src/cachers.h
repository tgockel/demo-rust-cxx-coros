#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

enum cachers_state {
  /**
   * No data is associated -- it will never arrive.
   */
  CACHERS_STATE_NONE,
  /**
   * The data has been fetched.
   */
  CACHERS_STATE_COMPLETE,
  CACHERS_STATE_IN_PROGRESS,
  CACHERS_STATE_ERROR,
};

enum cachers_err {
  CACHERS_ERR_OK = 0,
  CACHERS_ERR_NOT_IMPLEMENTED,
  CACHERS_ERR_INVALID_ARGUMENT,
  CACHERS_ERR_EMPTY,
  CACHERS_ERR_HAS_DATA,
};

/**
 * The database.
 */
struct cachers_db;

struct cachers_response_token;

struct cachers_response {
  struct cachers_response_token *token;
  enum cachers_err error_code;
  const void *header;
  size_t header_size;
  enum cachers_state data_state;
  const void *data;
  size_t data_size;
};

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

const char *cachers_current_errstr(void);

enum cachers_err cachers_open(struct cachers_db **out);

enum cachers_err cachers_release(struct cachers_db *db);

enum cachers_err cachers_get(struct cachers_db *db,
                             const void *key,
                             size_t key_len,
                             struct cachers_response *out);

enum cachers_err cachers_response_get_or_bind(struct cachers_response_token *token,
                                              void (*callback)(const struct cachers_response *response,
                                                               void *cxt),
                                              void *callback_cxt,
                                              struct cachers_response *maybe_out);

enum cachers_err cachers_response_token_release(struct cachers_response_token *token);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus
