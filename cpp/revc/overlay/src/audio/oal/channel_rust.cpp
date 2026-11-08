#include "common.h"

#if defined(AUDIO_OAL) && defined(AUDIO_RUST_SFX)
// CChannel backed by the Rust mixer (rust/revc/src/audio): sound-effect voices are mixed in Rust
// and played through SDL3. Streams (music, radio, cutscene audio) still use OpenAL
// (oal/stream.cpp) until they move too. Build with -DREVC_RUST_SFX=OFF to use channel.cpp
// (OpenAL voices) instead, e.g. to compare by ear.
#include "channel.h"
#include "sampman.h"
#include <float.h>

extern "C" {
int revc_sfx_open(unsigned int voices);
void revc_sfx_close(void);
void revc_sfx_set_2d(unsigned int id, int is2d);
void revc_sfx_start(unsigned int id, const short *data, size_t samples, int freq);
void revc_sfx_stop(unsigned int id);
void revc_sfx_reset(unsigned int id);
int revc_sfx_is_playing(unsigned int id);
void revc_sfx_set_pitch(unsigned int id, float pitch);
void revc_sfx_set_gain(unsigned int id, float gain);
void revc_sfx_set_position(unsigned int id, float x, float y, float z);
void revc_sfx_set_distances(unsigned int id, float max, float min);
void revc_sfx_set_loop_points(unsigned int id, int start, int end);
void revc_sfx_set_loop_count(unsigned int id, int count);
void revc_sfx_set_reverb_mix(unsigned int id, float mix);
}

// Looping with a count is handled inside the mixer, so no channel ever needs Service().
int32 CChannel::channelsThatNeedService = 0;

static bool bChannelsOpened = false;

void
CChannel::InitChannels()
{
	bChannelsOpened = revc_sfx_open(NUM_CHANNELS) != 0;
}

void
CChannel::DestroyChannels()
{
	if (bChannelsOpened) {
		revc_sfx_close();
		bChannelsOpened = false;
	}
}

CChannel::CChannel()
{
	Data = nil;
	DataSize = 0;
	bIs2D = false;
	SetDefault();
}

void CChannel::SetDefault()
{
	Pitch = 1.0f;
	Gain = 1.0f;
	Mix = 0.0f;

	Position[0] = 0.0f; Position[1] = 0.0f; Position[2] = 0.0f;
	Distances[0] = 0.0f; Distances[1] = FLT_MAX;

	LoopCount = 1;
	LastProcessedOffset = UINT32_MAX;
	LoopPoints[0] = 0; LoopPoints[1] = -1;

	Frequency = MAX_FREQ;
}

void CChannel::Reset()
{
	ClearBuffer();
	SetDefault();
}

void CChannel::Init(uint32 _id, bool Is2D)
{
	id = _id;
	if ( HasSource() && Is2D )
	{
		bIs2D = true;
		revc_sfx_set_2d(id, 1);
		revc_sfx_set_position(id, 0.0f, 0.0f, 0.0f);
		revc_sfx_set_gain(id, 1.0f);
	}
}

void CChannel::Term()
{
	Stop();
}

void CChannel::Start()
{
	if ( !HasSource() ) return;
	if ( !Data ) return;
	// The mixer copies the samples (like alBufferData), so ped-comment buffers can be reused.
	revc_sfx_set_loop_points(id, LoopPoints[0], LoopPoints[1]);
	revc_sfx_start(id, (const short *)Data, DataSize / 2, Frequency);
}

void CChannel::Stop()
{
	if ( HasSource() )
		revc_sfx_stop(id);
	Reset();
}

bool CChannel::HasSource()
{
	return bChannelsOpened;
}

bool CChannel::IsUsed()
{
	return HasSource() && revc_sfx_is_playing(id);
}

void CChannel::SetPitch(float pitch)
{
	if ( !HasSource() ) return;
	revc_sfx_set_pitch(id, pitch);
}

void CChannel::SetGain(float gain)
{
	if ( !HasSource() ) return;
	revc_sfx_set_gain(id, gain);
}

void CChannel::SetVolume(int32 vol)
{
	SetGain(float(vol) / MAX_VOLUME);
}

void CChannel::SetSampleData(void *_data, size_t _DataSize, int32 freq)
{
	Data = _data;
	DataSize = _DataSize;
	Frequency = freq;
}

void CChannel::SetCurrentFreq(uint32 freq)
{
	SetPitch(float(freq) / Frequency);
}

void CChannel::SetLoopCount(int32 count)
{
	if ( !HasSource() ) return;
	// 0: loop indefinitely, 1: play one time, 2: play two times etc.
	LoopCount = count;
	revc_sfx_set_loop_count(id, count);
}

bool CChannel::Update()
{
	return false;
}

void CChannel::SetLoopPoints(ALint start, ALint end)
{
	LoopPoints[0] = start;
	LoopPoints[1] = end;
}

void CChannel::SetPosition(float x, float y, float z)
{
	if ( !HasSource() ) return;
	revc_sfx_set_position(id, x, y, z);
}

void CChannel::SetDistances(float max, float min)
{
	if ( !HasSource() ) return;
	revc_sfx_set_distances(id, max, min);
}

void CChannel::SetPan(int32 pan)
{
	SetPosition((pan-63)/64.0f, 0.0f, Sqrt(1.0f-SQR((pan-63)/64.0f)));
}

void CChannel::ClearBuffer()
{
	if ( !HasSource() ) return;
	revc_sfx_reset(id);
	Data = nil;
	DataSize = 0;
}

void CChannel::SetReverbMix(ALuint slot, float mix)
{
	if ( !HasSource() ) return;
	Mix = mix;
	revc_sfx_set_reverb_mix(id, mix);
}

void CChannel::UpdateReverb(ALuint slot)
{
}

#endif
