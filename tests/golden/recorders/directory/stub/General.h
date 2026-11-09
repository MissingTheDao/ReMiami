#pragma once
#include <ctype.h>
struct CGeneral {
	static bool faststricmp(const char *str1, const char *str2) {
		for (; *str1; str1++, str2++) if (toupper(*str1) != toupper(*str2)) return true;
		return *str2 != '\0';
	}
};
