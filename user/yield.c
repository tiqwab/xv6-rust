#include "user.h"

void umain(int argc, char **argv) {
	int i;

    int env_id = sys_get_env_id();

	printf("Hello, I am environment %08x.\n", env_id);
	for (i = 0; i < 5; i++) {
		sys_yield();
		printf("Back in environment %08x, iteration %d.\n", env_id, i);
	}
	printf("All done in environment %08x.\n", env_id);
}
