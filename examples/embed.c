#include "temiya.h"
#include <stdio.h>

int main(void) {
  TemiyaEngine *engine = NULL;
  TemiyaRealm *realm = NULL;
  TemiyaScript *script = NULL;
  TemiyaValue *value = NULL;
  TemiyaError *error = NULL;
  double number = 0;
  int result = 1;

  if (temiya_abi_version() != TEMIYA_ABI_VERSION ||
      temiya_engine_new(&engine, &error) != TEMIYA_OK ||
      temiya_realm_new(engine, &realm, &error) != TEMIYA_OK ||
      temiya_script_compile(engine, realm, "40 + 2;", &script, &error) !=
          TEMIYA_OK ||
      temiya_script_evaluate(engine, script, &value, &error) != TEMIYA_OK ||
      temiya_value_to_number(value, &number) != TEMIYA_OK) {
    fprintf(stderr, "%s\n", error ? temiya_error_message(error) : "C ABI error");
    goto cleanup;
  }
  printf("%.0f\n", number);
  result = 0;

cleanup:
  temiya_error_free(error);
  temiya_value_free(value);
  temiya_script_free(script);
  temiya_realm_free(realm);
  temiya_engine_free(engine);
  return result;
}
