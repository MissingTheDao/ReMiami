// Runs the original GetSaveType + FixSave on each file given (copied to <dir>/GTAVCsf1.b) and
// prints: name type fnv64(fixed file) length.
#include "common.h"
#define FIX_INCOMPATIBLE_SAVES
#include "fix.inc"
#include <string>
static uint64_t fnv(const std::string &p, long *len) {
	FILE *f = fopen(p.c_str(), "rb"); uint64_t h = 0xcbf29ce484222325ull; int c; *len = 0;
	while ((c = fgetc(f)) != EOF) { h = (h ^ (uint8)c) * 0x100000001b3ull; ++*len; }
	fclose(f); return h;
}
int main(int argc, char **argv) {
	std::string dir = argv[1];
	snprintf(DefaultPCSaveFileName, MAX_PATH, "%s/GTAVCsf", dir.c_str());
	for (int i = 2; i < argc; i++) {
		std::string slot = dir + "/GTAVCsf1.b";
		std::string cmd = "cp '" + std::string(argv[i]) + "' '" + slot + "'"; if (system(cmd.c_str())) return 1;
		char name[MAX_PATH]; strcpy(name, slot.c_str());
		uint8 t = GetSaveType(name);
		bool ok = t == (SAVE_TYPE_32_BIT | SAVE_TYPE_MSVC) || FixSave(0, t);
		long len; uint64_t h = fnv(slot, &len);
		const char *base = strrchr(argv[i], '/'); base = base ? base + 1 : argv[i];
		printf("%s %d %d %016llx %ld\n", base, t, ok, (unsigned long long)h, len);
		system(("rm -f '" + dir + "'/*.bak").c_str());
	}
}
