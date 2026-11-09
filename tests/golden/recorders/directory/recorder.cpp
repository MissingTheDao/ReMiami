// Runs the original CDirectory on synthetic .dir files (written to tests/golden/directory/):
// ReadDirFile, AddItem(info, 3) of "extra.txd", then FindItem for some names (other case,
// duplicates, missing). One line per file: "file max queries... => count entries... answers...".
#include "common.h"
#include <stdio.h>
#include <stdint.h>
#include <string>
#include <vector>
#include "FileMgr.h"
#include "General.h"
#include "Directory.h"
#include "directory.inc"

static uint32 rng = 0xd1ec7012u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static std::string name() {
	static const char *ext[] = { ".dff", ".txd", ".ifp", ".col" };
	char b[40]; snprintf(b, sizeof b, next() % 3 ? "Model%u%s" : "objECT_%u%s", next() % 60, ext[next() % 4]);
	return b;
}
int main(int argc, char **argv) {
	for (int f = 0; f < 10; f++) {
		char path[512], file[32];
		snprintf(file, sizeof file, "%02d.dir", f);
		snprintf(path, sizeof path, "%s/%s", argv[1], file);
		std::vector<std::string> names;
		std::string data;
		int n = 5 + next() % 60;
		for (int i = 0; i < n; i++) {
			CDirectory::DirectoryInfo d; memset(&d, 0, sizeof d);
			d.offset = next(); d.size = next() % 5000;
			std::string nm = name(); names.push_back(nm);
			strncpy(d.name, nm.c_str(), 24);
			if (next() % 20 == 0) memset(d.name, 'Z', 23); // long name (23 chars + NUL)
			data.append((const char *)&d, sizeof d);
		}
		if (f % 3 == 1) data.append("PARTIALREC", 10); // a short last record
		FILE *fp = fopen(path, "wb"); fwrite(data.data(), 1, data.size(), fp); fclose(fp);
		int max = f % 4 == 0 ? n / 2 + 1 : n + 8; // sometimes too small: later records dropped
		// (The original asserts on a full directory and writes past it; keep the recorder in bounds.)
		if (max < n + 2) max = n + 2;
		std::vector<std::string> q;
		for (int k = 0; k < 8; k++) {
			std::string s = names[next() % names.size()];
			if (k % 2) for (auto &c : s) c = toupper(c);
			q.push_back(s);
		}
		q.push_back("missing.dff"); q.push_back("EXTRA.TXD"); q.push_back("ZZZZZZZZZZZZZZZZZZZZZZZ");
		CDirectory dir(max);
		dir.ReadDirFile(path);
		CDirectory::DirectoryInfo extra; memset(&extra, 0, sizeof extra);
		extra.offset = 0x1234; extra.size = 7; strcpy(extra.name, "extra.txd");
		dir.AddItem(extra, 3);
		printf("%s %d", file, max);
		for (auto &s : q) printf(" %s", s.c_str());
		printf(" => %d", dir.numEntries);
		for (int i = 0; i < dir.numEntries; i++) printf(" %08x:%08x", dir.entries[i].offset, dir.entries[i].size);
		for (auto &s : q) {
			uint32 o, sz;
			if (dir.FindItem(s.c_str(), o, sz)) printf(" %s=%08x:%08x", s.c_str(), o, sz); else printf(" %s=-", s.c_str());
		}
		printf("\n");
	}
}
