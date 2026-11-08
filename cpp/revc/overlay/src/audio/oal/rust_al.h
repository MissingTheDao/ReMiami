#pragma once

// Streams (music, radio, cutscene audio) on the Rust mixer (rust/revc-audio/src/al.rs).
// That module implements the OpenAL streaming-source subset oal/stream.cpp uses, so stream.cpp
// keeps its logic and only has its al* calls renamed here. Same AL types and enum values.
// Build with -DREVC_RUST_STREAMS=OFF to use real OpenAL for streams instead.

#if defined(AUDIO_OAL) && defined(AUDIO_RUST_STREAMS)
#include <AL/al.h>

#ifdef __cplusplus
extern "C" {
#endif
void revc_alGenSources(ALsizei n, ALuint *sources);
void revc_alDeleteSources(ALsizei n, const ALuint *sources);
void revc_alGenBuffers(ALsizei n, ALuint *buffers);
void revc_alDeleteBuffers(ALsizei n, const ALuint *buffers);
ALboolean revc_alIsBuffer(ALuint buffer);
void revc_alBufferData(ALuint buffer, ALenum format, const ALvoid *data, ALsizei size, ALsizei freq);
void revc_alSourceQueueBuffers(ALuint source, ALsizei n, const ALuint *buffers);
void revc_alSourceUnqueueBuffers(ALuint source, ALsizei n, ALuint *buffers);
void revc_alSourcePlay(ALuint source);
void revc_alSourcePause(ALuint source);
void revc_alSourceStop(ALuint source);
void revc_alGetSourcei(ALuint source, ALenum param, ALint *value);
void revc_alSourcei(ALuint source, ALenum param, ALint value);
void revc_alSourcef(ALuint source, ALenum param, ALfloat value);
void revc_alSource3f(ALuint source, ALenum param, ALfloat x, ALfloat y, ALfloat z);
#ifdef __cplusplus
}
#endif

#define alGenSources revc_alGenSources
#define alDeleteSources revc_alDeleteSources
#define alGenBuffers revc_alGenBuffers
#define alDeleteBuffers revc_alDeleteBuffers
#define alIsBuffer revc_alIsBuffer
#define alBufferData revc_alBufferData
#define alSourceQueueBuffers revc_alSourceQueueBuffers
#define alSourceUnqueueBuffers revc_alSourceUnqueueBuffers
#define alSourcePlay revc_alSourcePlay
#define alSourcePause revc_alSourcePause
#define alSourceStop revc_alSourceStop
#define alGetSourcei revc_alGetSourcei
#define alSourcei revc_alSourcei
#define alSourcef revc_alSourcef
#define alSource3f revc_alSource3f
#endif
