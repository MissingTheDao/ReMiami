namespace rw {

struct Atomic;

class Pipeline
{
public:
	uint32 pluginID;
	uint32 pluginData;
	int32 platform;

	void init(uint32 platform) {
		this->pluginID = 0;
		this->pluginData = 0;
		this->platform = platform;
	}
};

class ObjPipeline : public Pipeline
{
public:
	void init(uint32 platform);
	static ObjPipeline *create(void);	// always PLATFORM_NULL
	void destroy(void);

	// not the most beautiful way of doing things but still
	// better than virtual methods (i hope?).
	struct {
		void (*instance)(ObjPipeline *pipe, Atomic *atomic);
		void (*uninstance)(ObjPipeline *pipe, Atomic *atomic);
		void (*render)(ObjPipeline *pipe, Atomic *atomic);
	} impl;
	// just for convenience
	void instance(Atomic *atomic) { this->impl.instance(this, atomic); }
	void uninstance(Atomic *atomic) { this->impl.uninstance(this, atomic); }
	void render(Atomic *atomic) { this->impl.render(this, atomic); }
};


// everything xbox, d3d8 and d3d9 may want to use
enum {
	VERT_BYTE2 = 1,
	VERT_BYTE3,
	VERT_SHORT2,
	VERT_SHORT3,
	VERT_NORMSHORT2,
	VERT_NORMSHORT3,
	VERT_FLOAT2,
	VERT_FLOAT3,
	VERT_FLOAT4,
	VERT_ARGB,
	VERT_RGBA,
	VERT_COMPNORM
};


}
