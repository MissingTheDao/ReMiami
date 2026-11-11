#pragma once
namespace rw {

struct Error
{
	uint32 plugin;
	uint32 code;
};

// rust/revc/src/rwengine.rs
extern "C" void revc_set_error(Error *e);
extern "C" Error *revc_get_error(Error *e);
extern "C" const char *revc_error_format(uint32 code);
inline void setError(Error *e) { revc_set_error(e); }
inline Error *getError(Error *e) { return revc_get_error(e); }

#define _ERRORCODE(code, ...) code
#define _ERRORFORMAT(code, ...) rw::revc_error_format(code), ##__VA_ARGS__

/* ecode is supposed to be in format "(errorcode, printf-arguments..)" */
#define RWERROR(ecode) do{          \
	rw::Error _e;                   \
	_e.plugin = PLUGIN_ID;      \
	_e.code = _ERRORCODE ecode; \
	fprintf(stderr, "%s:%d: ", __FILE__, __LINE__); \
	fprintf(stderr, _ERRORFORMAT ecode); fputc('\n', stderr); \
	rw::setError(&_e);              \
}while(0)

}
