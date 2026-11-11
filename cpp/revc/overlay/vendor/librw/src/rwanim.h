#pragma once
#include <stddef.h>

namespace rw {

struct Animation;

// These sizes of these are sadly not platform independent
// because pointer sizes can vary.

struct KeyFrameHeader
{
	KeyFrameHeader *prev;
	float32 time;

	KeyFrameHeader *next(int32 sz){
		return (KeyFrameHeader*)((uint8*)this + sz); }
};

struct InterpFrameHeader
{
	KeyFrameHeader *keyFrame1;
	KeyFrameHeader *keyFrame2;
};

struct AnimInterpolatorInfo
{
	typedef void (*ApplyCB)(void *result, void *frame);
	typedef void (*BlendCB)(void *out, void *in1, void *in2, float32 a);
	typedef void (*InterpCB)(void *out, void *in1, void *in2, float32 t,
	                         void *custom);
	typedef void (*AddCB)(void *out, void *in1, void *in2);
	typedef void (*MulRecipCB)(void *frame, void *start);

	int32      id;
	int32      interpKeyFrameSize;
	int32      animKeyFrameSize;
	int32      customDataSize;

	ApplyCB    applyCB;
	BlendCB    blendCB;
	InterpCB   interpCB;
	AddCB      addCB;
	MulRecipCB mulRecipCB;
	void (*streamRead)(Stream *stream, Animation *anim);
	void (*streamWrite)(Stream *stream, Animation *anim);
	uint32 (*streamGetSize)(Animation *anim);

	static void registerInterp(AnimInterpolatorInfo *interpInfo);
	static void unregisterInterp(AnimInterpolatorInfo *interpInfo);
	static AnimInterpolatorInfo *find(int32 id);
};

struct Animation
{
	AnimInterpolatorInfo *interpInfo;
	int32    numFrames;
	int32    flags;
	float32  duration;
	void    *keyframes;
	void    *customData;

	static Animation *create(AnimInterpolatorInfo*, int32 numFrames,
	                         int32 flags, float duration);
	void destroy(void);
	KeyFrameHeader *getAnimFrame(int32 n){
		return (KeyFrameHeader*)((uint8*)this->keyframes +
		                         n*this->interpInfo->animKeyFrameSize);
	}
};

struct AnimInterpolator
{
	Animation *currentAnim;
	float32    currentTime;
	void      *nextFrame;
	int32      maxInterpKeyFrameSize;
	int32      currentInterpKeyFrameSize;
	int32      currentAnimKeyFrameSize;
	int32      numNodes;
	// TODO some callbacks, parent/sub
	// cached from the InterpolatorInfo
	AnimInterpolatorInfo::ApplyCB    applyCB;
	AnimInterpolatorInfo::BlendCB    blendCB;
	AnimInterpolatorInfo::InterpCB   interpCB;
	AnimInterpolatorInfo::AddCB      addCB;
	// after this interpolated frames

	static AnimInterpolator *create(int32 numNodes, int32 maxKeyFrameSize);
	void destroy(void);
	bool32 setCurrentAnim(Animation *anim);
	void addTime(float32 t);
	void *getFrames(void){ return this+1;}
	InterpFrameHeader *getInterpFrame(int32 n){
		return (InterpFrameHeader*)((uint8*)getFrames() +
		                            n*currentInterpKeyFrameSize);
	}
	KeyFrameHeader *getAnimFrame(int32 n){
		return (KeyFrameHeader*)((uint8*)currentAnim->keyframes +
		                         n*currentAnimKeyFrameSize);
	}
};

}
