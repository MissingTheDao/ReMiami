/* Measures OpenAL Soft's stereo output gains for a mono source at various positions (with the
 * game's listener setup: listener-relative sources, at = +Y, up = +Z, inverse-distance-clamped)
 * and for a 2D stereo buffer, so the Rust mixer can match them. Run via tests/calibrate/run.sh,
 * which uses OpenAL Soft's wave driver; this program just plays the test sequence.
 * Each step plays a 1 kHz tone for STEP_MS; run.sh measures per-step RMS of L and R. */
#include <AL/al.h>
#include <AL/alc.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define RATE 48000
#define STEP_MS 400
#define TONE_SAMPLES (RATE * STEP_MS / 1000)

static void sleep_ms(int ms) { struct timespec t = { ms / 1000, (ms % 1000) * 1000000L }; nanosleep(&t, NULL); }

int main(void)
{
	ALCdevice *dev = alcOpenDevice(NULL);
	if (!dev) { fprintf(stderr, "no device\n"); return 1; }
	ALCint attrs[] = { ALC_FREQUENCY, RATE, 0 };
	ALCcontext *ctx = alcCreateContext(dev, attrs);
	alcMakeContextCurrent(ctx);

	/* The game's listener and distance model (sampman_oal.cpp). */
	ALfloat orientation[6] = { 0.0f, 1.0f, 0.0f, 0.0f, 0.0f, 1.0f };
	alListenerfv(AL_ORIENTATION, orientation);
	alListener3f(AL_POSITION, 0, 0, 0);
	alDistanceModel(AL_INVERSE_DISTANCE_CLAMPED);

	static short mono[TONE_SAMPLES], stereo[TONE_SAMPLES * 2];
	for (int i = 0; i < TONE_SAMPLES; i++) {
		mono[i] = (short)(16384 * sin(2 * M_PI * 1000.0 * i / RATE));
		stereo[2 * i] = stereo[2 * i + 1] = mono[i];
	}
	ALuint bmono, bstereo, src;
	alGenBuffers(1, &bmono);
	alBufferData(bmono, AL_FORMAT_MONO16, mono, sizeof(mono), RATE);
	alGenBuffers(1, &bstereo);
	alBufferData(bstereo, AL_FORMAT_STEREO16, stereo, sizeof(stereo), RATE);
	alGenSources(1, &src);
	alSourcei(src, AL_SOURCE_RELATIVE, AL_TRUE);
	alSourcef(src, AL_REFERENCE_DISTANCE, 1.0f);

	/* Step list printed to stdout so run.sh can label the measurements. */
	printf("step label x y z buffer\n");
	int step = 0;
	/* Mono source on a circle of radius 1 (no attenuation), azimuth from -180 (behind) to 180. */
	for (int az = -180; az <= 180; az += 15) {
		float x = sinf(az * M_PI / 180), y = cosf(az * M_PI / 180);
		printf("%d az%+d %.4f %.4f 0 mono\n", step++, az, x, y);
	}
	/* The stream pan positions the game uses (CStream::SetPan) for left/right at centre pan. */
	printf("%d streamL %.4f 0 %.4f mono\n", step++, -63 / 64.0f, sqrtf(1 - (63 / 64.0f) * (63 / 64.0f)));
	printf("%d streamR 1 0 0 mono\n", step++);
	/* Distance: a centred source at 2 m and 4 m with reference 1 (expect 1/2 and 1/4). */
	printf("%d dist2 0 2 0 mono\n", step++);
	printf("%d dist4 0 4 0 mono\n", step++);
	/* 2D: stereo buffer at the origin (CChannel 2D voices). */
	printf("%d stereo2d 0 0 0 stereo\n", step++);
	fflush(stdout);

	/* Play them in the same order, with a gap of silence between steps. */
	sleep_ms(500);
	struct { float x, y, z; int stereo; } steps[64];
	int n = 0;
	for (int az = -180; az <= 180; az += 15)
		steps[n++] = (typeof(steps[0])){ sinf(az * M_PI / 180), cosf(az * M_PI / 180), 0, 0 };
	steps[n++] = (typeof(steps[0])){ -63 / 64.0f, 0, sqrtf(1 - (63 / 64.0f) * (63 / 64.0f)), 0 };
	steps[n++] = (typeof(steps[0])){ 1, 0, 0, 0 };
	steps[n++] = (typeof(steps[0])){ 0, 2, 0, 0 };
	steps[n++] = (typeof(steps[0])){ 0, 4, 0, 0 };
	steps[n++] = (typeof(steps[0])){ 0, 0, 0, 1 };
	for (int i = 0; i < n; i++) {
		alSourcei(src, AL_BUFFER, 0);
		alSource3f(src, AL_POSITION, steps[i].x, steps[i].y, steps[i].z);
		alSourcei(src, AL_BUFFER, steps[i].stereo ? bstereo : bmono);
		alSourcePlay(src);
		sleep_ms(STEP_MS + 200); /* tone + 200 ms gap */
		alSourceStop(src);
	}
	sleep_ms(300);
	alcMakeContextCurrent(NULL);
	alcDestroyContext(ctx);
	alcCloseDevice(dev);
	return 0;
}
