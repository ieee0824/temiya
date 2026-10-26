#ifndef TEMIYA_H
#define TEMIYA_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TEMIYA_ABI_VERSION 1u

typedef struct TemiyaEngine TemiyaEngine;
typedef struct TemiyaRealm TemiyaRealm;
typedef struct TemiyaScript TemiyaScript;
typedef struct TemiyaValue TemiyaValue;
typedef struct TemiyaError TemiyaError;

typedef enum TemiyaStatus {
  TEMIYA_OK = 0,
  TEMIYA_INVALID_ARGUMENT = 1,
  TEMIYA_EXCEPTION = 2,
  TEMIYA_PANIC = 3
} TemiyaStatus;

uint32_t temiya_abi_version(void);
TemiyaStatus temiya_engine_new(TemiyaEngine **out, TemiyaError **error);
void temiya_engine_free(TemiyaEngine *engine);
TemiyaStatus temiya_realm_new(TemiyaEngine *engine, TemiyaRealm **out,
                              TemiyaError **error);
void temiya_realm_free(TemiyaRealm *realm);
TemiyaStatus temiya_script_compile(TemiyaEngine *engine, TemiyaRealm *realm,
                                   const char *source, TemiyaScript **out,
                                   TemiyaError **error);
void temiya_script_free(TemiyaScript *script);
TemiyaStatus temiya_script_evaluate(TemiyaEngine *engine, TemiyaScript *script,
                                    TemiyaValue **out, TemiyaError **error);
void temiya_value_free(TemiyaValue *value);
TemiyaStatus temiya_value_to_number(const TemiyaValue *value, double *out);
const char *temiya_error_message(const TemiyaError *error);
void temiya_error_free(TemiyaError *error);

#ifdef __cplusplus
}
#endif
#endif
