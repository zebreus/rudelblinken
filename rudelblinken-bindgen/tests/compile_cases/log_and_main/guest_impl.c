#include "generated_c_guest.h"

int main(void) {
    char message[] = "hello from wasm";
    host_log(message);
    return 0;
}
