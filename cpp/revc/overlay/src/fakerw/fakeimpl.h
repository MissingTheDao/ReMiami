// revc (docs/RENDERER_PORT.md, stage 2 step 7d4): the RW API the game is written against, as
// inline adapters over librw's (Rust-backed) classes; fake.cpp is gone. Included at the end of
// rwcore.h, so every file using the RW API has the definitions. The adapters move to Rust with
// their callers when the game code is ported (stages 4 and 5).
#pragma once
#include <string.h>
#include <assert.h>
#include <rpworld.h>
#include <rpmatfx.h>
#include <rphanim.h>
#include <rpskin.h>
#include <rtcharse.h>
#include <rtquat.h>
#include <rtbmp.h>
#include <rtpng.h>
#include <rpanisot.h>
#ifndef _WIN32
// skel/crossplatform.h (no include guard): declared here without its default argument
char *casepath(char const *path, bool checkPathFirst);
#endif

inline RwUInt8 RwObjectGetType(const RwObject *obj) { using namespace rw; return obj->type; }
inline RwFrame* rwObjectGetParent(const RwObject *obj) { using namespace rw; return (RwFrame*)obj->parent; }

inline void *RwMalloc(size_t size) { using namespace rw; return engine->memfuncs.rwmalloc(size, 0); }
inline void *RwCalloc(size_t numObj, size_t sizeObj) { using namespace rw;
	void *mem = RwMalloc(numObj*sizeObj);
	if(mem)
		memset(mem, 0, numObj*sizeObj);
	return mem;
}
// Like free(NULL), freeing nothing is a no-op: static objects (e.g. CColModel globals) free their
// nil pointers at process exit, which in unit-test binaries happens without an RW engine.
inline void  RwFree(void *mem) { using namespace rw; if(mem) engine->memfuncs.rwfree(mem); }


//RwReal RwV3dNormalize(RwV3d * out, const RwV3d * in);
inline RwReal RwV3dLength(const RwV3d * in) { using namespace rw; return length(*in); }
//RwReal RwV2dLength(const RwV2d * in);
//RwReal RwV2dNormalize(RwV2d * out, const RwV2d * in);
//void RwV2dAssign(RwV2d * out, const RwV2d * ina);
//void RwV2dAdd(RwV2d * out, const RwV2d * ina, const RwV2d * inb);
//void RwV2dLineNormal(RwV2d * out, const RwV2d * ina, const RwV2d * inb);
//void RwV2dSub(RwV2d * out, const RwV2d * ina, const RwV2d * inb);
//void RwV2dPerp(RwV2d * out, const RwV2d * in);
//void RwV2dScale(RwV2d * out, const RwV2d * in, RwReal scalar);
//RwReal RwV2dDotProduct(const RwV2d * ina, const RwV2d * inb);
//void RwV3dAssign(RwV3d * out, const RwV3d * ina);
inline void RwV3dAdd(RwV3d * out, const RwV3d * ina, const RwV3d * inb) { using namespace rw; *out = add(*ina, *inb); }
inline void RwV3dSub(RwV3d * out, const RwV3d * ina, const RwV3d * inb) { using namespace rw; *out = sub(*ina, *inb); }
inline void RwV3dScale(RwV3d * out, const RwV3d * in, RwReal scalar) { using namespace rw; *out = scale(*in, scalar); }
inline void RwV3dIncrementScaled(RwV3d * out,  const RwV3d * in, RwReal scalar) { using namespace rw; *out = add(*out, scale(*in, scalar)); }
inline void RwV3dNegate(RwV3d * out, const RwV3d * in) { using namespace rw; *out = neg(*in); }
inline RwReal RwV3dDotProduct(const RwV3d * ina, const RwV3d * inb) { using namespace rw; return dot(*ina, *inb); }
//void RwV3dCrossProduct(RwV3d * out, const RwV3d * ina, const RwV3d * inb);
inline RwV3d *RwV3dTransformPoints(RwV3d * pointsOut, const RwV3d * pointsIn, RwInt32 numPoints, const RwMatrix * matrix)
	{ using namespace rw; V3d::transformPoints(pointsOut, pointsIn, numPoints, matrix); return pointsOut; }
//RwV3d *RwV3dTransformVectors(RwV3d * vectorsOut, const RwV3d * vectorsIn, RwInt32 numPoints, const RwMatrix * matrix);



inline RwBool RwMatrixDestroy(RwMatrix *mpMat) { using namespace rw; mpMat->destroy(); return true; }
inline RwMatrix *RwMatrixCreate(void) { using namespace rw; return Matrix::create(); }
inline void RwMatrixCopy(RwMatrix * dstMatrix, const RwMatrix * srcMatrix) { using namespace rw; *dstMatrix = *srcMatrix; }
inline void RwMatrixSetIdentity(RwMatrix * matrix) { using namespace rw; matrix->setIdentity(); }
RwMatrix *RwMatrixMultiply(RwMatrix * matrixOut, const RwMatrix * MatrixIn1, const RwMatrix * matrixIn2);
inline RwMatrix *RwMatrixTransform(RwMatrix * matrix, const RwMatrix * transform, RwOpCombineType combineOp)
	{ using namespace rw; matrix->transform(transform, (rw::CombineOp)combineOp); return matrix; }
//RwMatrix *RwMatrixOrthoNormalize(RwMatrix * matrixOut, const RwMatrix * matrixIn);
inline RwMatrix *RwMatrixInvert(RwMatrix * matrixOut, const RwMatrix * matrixIn) { using namespace rw; Matrix::invert(matrixOut, matrixIn); return matrixOut; }
inline RwMatrix *RwMatrixScale(RwMatrix * matrix, const RwV3d * scale, RwOpCombineType combineOp)
	{ using namespace rw; matrix->scale(scale, (rw::CombineOp)combineOp); return matrix; }
inline RwMatrix *RwMatrixTranslate(RwMatrix * matrix, const RwV3d * translation, RwOpCombineType combineOp)
	{ using namespace rw; matrix->translate(translation, (rw::CombineOp)combineOp); return matrix; }
inline RwMatrix *RwMatrixRotate(RwMatrix * matrix, const RwV3d * axis, RwReal angle, RwOpCombineType combineOp)
	{ using namespace rw; matrix->rotate(axis, angle, (rw::CombineOp)combineOp); return matrix; }
//RwMatrix *RwMatrixRotateOneMinusCosineSine(RwMatrix * matrix, const RwV3d * unitAxis, RwReal oneMinusCosine, RwReal sine, RwOpCombineType combineOp);
//const RwMatrix *RwMatrixQueryRotate(const RwMatrix * matrix, RwV3d * unitAxis, RwReal * angle, RwV3d * center);
inline RwV3d *RwMatrixGetRight(RwMatrix * matrix) { using namespace rw; return &matrix->right; }
inline RwV3d *RwMatrixGetUp(RwMatrix * matrix) { using namespace rw; return &matrix->up; }
inline RwV3d *RwMatrixGetAt(RwMatrix * matrix) { using namespace rw; return &matrix->at; }
inline RwV3d *RwMatrixGetPos(RwMatrix * matrix) { using namespace rw; return &matrix->pos; }
inline RwMatrix *RwMatrixUpdate(RwMatrix * matrix) { using namespace rw; matrix->update(); return matrix; }
//RwMatrix *RwMatrixOptimize(RwMatrix * matrix, const RwMatrixTolerance *tolerance);




inline RwFrame *RwFrameForAllObjects(RwFrame * frame, RwObjectCallBack callBack, void *data) { using namespace rw;
	FORLIST(lnk, frame->objectList)
		if(callBack(&ObjectWithFrame::fromFrame(lnk)->object, data) == nil)
			break;
	return frame;
}
inline RwFrame *RwFrameTranslate(RwFrame * frame, const RwV3d * v, RwOpCombineType combine) { using namespace rw; frame->translate(v, (CombineOp)combine); return frame; }
inline RwFrame *RwFrameRotate(RwFrame * frame, const RwV3d * axis, RwReal angle, RwOpCombineType combine) { using namespace rw; frame->rotate(axis, angle, (CombineOp)combine); return frame; }
inline RwFrame *RwFrameScale(RwFrame * frame, const RwV3d * v, RwOpCombineType combine) { using namespace rw; frame->scale(v, (CombineOp)combine); return frame; }
inline RwFrame *RwFrameTransform(RwFrame * frame, const RwMatrix * m, RwOpCombineType combine) { using namespace rw; frame->transform(m, (CombineOp)combine); return frame; }
//TODO: actually implement this!
inline RwFrame *RwFrameOrthoNormalize(RwFrame * frame) { using namespace rw; return frame; }
inline RwFrame *RwFrameSetIdentity(RwFrame * frame) { using namespace rw; frame->matrix.setIdentity(); frame->updateObjects(); return frame; }
//RwFrame *RwFrameCloneHierarchy(RwFrame * root);
//RwBool RwFrameDestroyHierarchy(RwFrame * frame);
inline RwFrame *RwFrameForAllChildren(RwFrame * frame, RwFrameCallBack callBack, void *data)
	{ using namespace rw; return frame->forAllChildren(callBack, data); }
inline RwFrame *RwFrameRemoveChild(RwFrame * child) { using namespace rw; child->removeChild(); return child; }
inline RwFrame *RwFrameAddChild(RwFrame * parent, RwFrame * child) { using namespace rw; parent->addChild(child); return parent; }
inline RwFrame *RwFrameGetParent(const RwFrame * frame) { using namespace rw; return frame->getParent(); }
//RwFrame *RwFrameGetRoot(const RwFrame * frame);
inline RwMatrix *RwFrameGetLTM(RwFrame * frame) { using namespace rw; return frame->getLTM(); }
inline RwMatrix *RwFrameGetMatrix(RwFrame * frame) { using namespace rw; return &frame->matrix; }
inline RwFrame *RwFrameUpdateObjects(RwFrame * frame) { using namespace rw; frame->updateObjects(); return frame; }
inline RwFrame *RwFrameCreate(void) { using namespace rw; return rw::Frame::create(); }
//RwBool RwFrameInit(RwFrame *frame);
//RwBool RwFrameDeInit(RwFrame *frame);
inline RwBool RwFrameDestroy(RwFrame * frame) { using namespace rw; frame->destroy(); return true; }
//void _rwFrameInit(RwFrame *frame);
//void _rwFrameDeInit(RwFrame *frame);
//RwBool RwFrameDirty(const RwFrame * frame);
//RwInt32 RwFrameCount(RwFrame * frame);
//RwBool RwFrameSetStaticPluginsSize(RwInt32 size);
inline RwInt32 RwFrameRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB)
	{ using namespace rw; return Frame::registerPlugin(size, pluginID, constructCB, destructCB, (CopyConstructor)copyCB); }
//RwInt32 RwFrameGetPluginOffset(RwUInt32 pluginID);
//RwBool RwFrameValidatePlugins(const RwFrame * frame);
//RwFrame *_rwFrameCloneAndLinkClones(RwFrame * root);
//RwFrame *_rwFramePurgeClone(RwFrame *root);

inline RwInt32 RwFrameRegisterPluginStream(RwUInt32 pluginID, RwPluginDataChunkReadCallBack readCB, RwPluginDataChunkWriteCallBack writeCB, RwPluginDataChunkGetSizeCallBack getSizeCB)
	{ using namespace rw; return Frame::registerPluginStream(pluginID, readCB, (StreamWrite)writeCB, (StreamGetSize)getSizeCB); }


// rwFrameListStreamRead: models are read in Rust (rw/DffRead.cpp)




inline RwCamera    *RwCameraBeginUpdate(RwCamera * camera) { using namespace rw; camera->beginUpdate(); return camera; }
inline RwCamera    *RwCameraEndUpdate(RwCamera * camera) { using namespace rw; camera->endUpdate(); return camera; }
inline RwCamera    *RwCameraClear(RwCamera * camera, RwRGBA * colour, RwInt32 clearMode) { using namespace rw; camera->clear(colour, clearMode); return camera; }
// WARNING: ignored argument
inline RwCamera    *RwCameraShowRaster(RwCamera * camera, void *pDev, RwUInt32 flags) { using namespace rw; camera->showRaster(flags); return camera; }
inline RwBool       RwCameraDestroy(RwCamera * camera) { using namespace rw; camera->destroy(); return true; }
inline RwCamera    *RwCameraCreate(void) { using namespace rw; return rw::Camera::create(); }
inline RwCamera    *RwCameraClone(RwCamera * camera) { using namespace rw; return camera->clone(); }
inline RwCamera    *RwCameraSetViewOffset(RwCamera *camera, const RwV2d *offset) { using namespace rw; camera->setViewOffset(offset); return camera; }
inline RwCamera    *RwCameraSetViewWindow(RwCamera *camera, const RwV2d *viewWindow) { using namespace rw; camera->setViewWindow(viewWindow); return camera; }
inline RwCamera    *RwCameraSetProjection(RwCamera *camera, RwCameraProjection projection) { using namespace rw; camera->projection = projection; return camera; }
inline RwCamera    *RwCameraSetNearClipPlane(RwCamera *camera, RwReal nearClip) { using namespace rw; camera->setNearPlane(nearClip); return camera; }
inline RwCamera    *RwCameraSetFarClipPlane(RwCamera *camera, RwReal farClip) { using namespace rw; camera->setFarPlane(farClip); return camera; }
RwInt32      RwCameraRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
RwInt32      RwCameraGetPluginOffset(RwUInt32 pluginID);
RwBool       RwCameraValidatePlugins(const RwCamera * camera);
inline RwFrustumTestResult RwCameraFrustumTestSphere(const RwCamera * camera, const RwSphere * sphere) { using namespace rw; return (RwFrustumTestResult)camera->frustumTestSphere(sphere); }
inline const RwV2d *RwCameraGetViewOffset(const RwCamera *camera) { using namespace rw; return &camera->viewOffset; }
inline RwCamera    *RwCameraSetRaster(RwCamera *camera, RwRaster *raster) { using namespace rw; camera->frameBuffer = raster; return camera; }
inline RwRaster    *RwCameraGetRaster(const RwCamera *camera) { using namespace rw; return camera->frameBuffer; }
inline RwCamera    *RwCameraSetZRaster(RwCamera *camera, RwRaster *zRaster) { using namespace rw; camera->zBuffer = zRaster; return camera; }
inline RwRaster    *RwCameraGetZRaster(const RwCamera *camera) { using namespace rw; return camera->zBuffer; }
inline RwReal       RwCameraGetNearClipPlane(const RwCamera *camera) { using namespace rw; return camera->nearPlane; }
inline RwReal       RwCameraGetFarClipPlane(const RwCamera *camera) { using namespace rw; return camera->farPlane; }
inline RwCamera    *RwCameraSetFogDistance(RwCamera *camera, RwReal fogDistance) { using namespace rw; camera->fogPlane = fogDistance; return camera; }
inline RwReal       RwCameraGetFogDistance(const RwCamera *camera) { using namespace rw; return camera->fogPlane; }
inline RwCamera    *RwCameraGetCurrentCamera(void) { using namespace rw; return rw::engine->currentCamera; }
RwCameraProjection RwCameraGetProjection(const RwCamera *camera);
inline const RwV2d *RwCameraGetViewWindow(const RwCamera *camera) { using namespace rw; return &camera->viewWindow; }
inline RwMatrix    *RwCameraGetViewMatrix(RwCamera *camera) { using namespace rw; return &camera->viewMatrix; }
inline RwCamera    *RwCameraSetFrame(RwCamera *camera, RwFrame *frame) { using namespace rw; camera->setFrame(frame); return camera; }
inline RwFrame     *RwCameraGetFrame(const RwCamera *camera) { using namespace rw; return camera->getFrame(); }





inline RwImage     *RwImageCreate(RwInt32 width, RwInt32 height, RwInt32 depth) { using namespace rw; return Image::create(width, height, depth); }
inline RwBool       RwImageDestroy(RwImage * image) { using namespace rw; image->destroy(); return true; }
inline RwImage     *RwImageAllocatePixels(RwImage * image) { using namespace rw; image->allocate(); return image; }
inline RwImage     *RwImageFreePixels(RwImage * image) { using namespace rw; image->free(); return image; }
RwImage     *RwImageCopy(RwImage * destImage, const RwImage * sourceImage);
RwImage     *RwImageResize(RwImage * image, RwInt32 width, RwInt32 height);
RwImage     *RwImageApplyMask(RwImage * image, const RwImage * mask);
RwImage     *RwImageMakeMask(RwImage * image);
RwImage     *RwImageReadMaskedImage(const RwChar * imageName, const RwChar * maskname);
RwImage     *RwImageRead(const RwChar * imageName);
RwImage     *RwImageWrite(RwImage * image, const RwChar * imageName);
RwChar      *RwImageGetPath(void);
inline const RwChar *RwImageSetPath(const RwChar * path) { using namespace rw; Image::setSearchPath(path); return path; }
inline RwImage     *RwImageSetStride(RwImage * image, RwInt32 stride) { using namespace rw; image->stride = stride; return image; }
inline RwImage     *RwImageSetPixels(RwImage * image, RwUInt8 * pixels) { using namespace rw; image->pixels = pixels; return image; }
inline RwImage     *RwImageSetPalette(RwImage * image, RwRGBA * palette) { using namespace rw; image->palette = (uint8*)palette; return image; }
inline RwInt32      RwImageGetWidth(const RwImage * image) { using namespace rw; return image->width; }
inline RwInt32      RwImageGetHeight(const RwImage * image) { using namespace rw; return image->height; }
inline RwInt32      RwImageGetDepth(const RwImage * image) { using namespace rw; return image->depth; }
inline RwInt32      RwImageGetStride(const RwImage * image) { using namespace rw; return image->stride; }
inline RwUInt8     *RwImageGetPixels(const RwImage * image) { using namespace rw; return image->pixels; }
inline RwRGBA      *RwImageGetPalette(const RwImage * image) { using namespace rw; return (RwRGBA*)image->palette; }
RwUInt32     RwRGBAToPixel(RwRGBA * rgbIn, RwInt32 rasterFormat);
RwRGBA      *RwRGBASetFromPixel(RwRGBA * rgbOut, RwUInt32 pixelValue, RwInt32 rasterFormat);
RwBool       RwImageSetGamma(RwReal gammaValue);
RwReal       RwImageGetGamma(void);
RwImage     *RwImageGammaCorrect(RwImage * image);
RwRGBA      *RwRGBAGammaCorrect(RwRGBA * rgb);
RwInt32      RwImageRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
RwInt32      RwImageGetPluginOffset(RwUInt32 pluginID);
RwBool       RwImageValidatePlugins(const RwImage * image);
//RwBool       RwImageRegisterImageFormat(const RwChar * extension, RwImageCallBackRead imageRead, RwImageCallBackWrite imageWrite);
const RwChar *RwImageFindFileType(const RwChar * imageName);
RwInt32      RwImageStreamGetSize(const RwImage * image);
RwImage     *RwImageStreamRead(RwStream * stream);
const RwImage *RwImageStreamWrite(const RwImage * image, RwStream * stream);

inline RwImage *RwImageFindRasterFormat(RwImage *ipImage,RwInt32 nRasterType, RwInt32 *npWidth,RwInt32 *npHeight, RwInt32 *npDepth,RwInt32 *npFormat)
{ using namespace rw;
	return Raster::imageFindRasterFormat(ipImage, nRasterType, npWidth, npHeight, npDepth, npFormat) ? ipImage : nil;
}




inline RwRaster    *RwRasterCreate(RwInt32 width, RwInt32 height, RwInt32 depth, RwInt32 flags) { using namespace rw; return Raster::create(width, height, depth, flags); }
inline RwBool       RwRasterDestroy(RwRaster * raster) { using namespace rw; raster->destroy(); return true; }
inline RwInt32      RwRasterGetWidth(const RwRaster *raster) { using namespace rw; return raster->width; }
inline RwInt32      RwRasterGetHeight(const RwRaster *raster) { using namespace rw; return raster->height; }
RwInt32      RwRasterGetStride(const RwRaster *raster);
inline RwInt32      RwRasterGetDepth(const RwRaster *raster) { using namespace rw; return raster->depth; }
RwInt32      RwRasterGetFormat(const RwRaster *raster);
RwInt32      RwRasterGetType(const RwRaster *raster);
inline RwRaster    *RwRasterGetParent(const RwRaster *raster) { using namespace rw; return raster->parent; }
RwRaster    *RwRasterGetOffset(RwRaster *raster,  RwInt16 *xOffset, RwInt16 *yOffset);
RwInt32      RwRasterGetNumLevels(RwRaster * raster);
RwRaster    *RwRasterSubRaster(RwRaster * subRaster, RwRaster * raster, RwRect * rect);
inline RwRaster    *RwRasterRenderFast(RwRaster * raster, RwInt32 x, RwInt32 y) { using namespace rw; return raster->renderFast(x, y) ? raster : nil; }
RwRaster    *RwRasterRender(RwRaster * raster, RwInt32 x, RwInt32 y);
RwRaster    *RwRasterRenderScaled(RwRaster * raster, RwRect * rect);
inline RwRaster    *RwRasterPushContext(RwRaster * raster) { using namespace rw; return Raster::pushContext(raster); }
inline RwRaster    *RwRasterPopContext(void) { using namespace rw; return Raster::popContext(); }
inline RwRaster    *RwRasterGetCurrentContext(void) { using namespace rw; return Raster::getCurrentContext(); }
RwBool       RwRasterClear(RwInt32 pixelValue);
RwBool       RwRasterClearRect(RwRect * rpRect, RwInt32 pixelValue);
RwRaster    *RwRasterShowRaster(RwRaster * raster, void *dev, RwUInt32 flags);
inline RwUInt8     *RwRasterLock(RwRaster * raster, RwUInt8 level, RwInt32 lockMode) { using namespace rw; return raster->lock(level, lockMode); }
inline RwRaster    *RwRasterUnlock(RwRaster * raster) { using namespace rw; raster->unlock(0); return raster; }
RwUInt8     *RwRasterLockPalette(RwRaster * raster, RwInt32 lockMode);
RwRaster    *RwRasterUnlockPalette(RwRaster * raster);
RwInt32      RwRasterRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
RwInt32      RwRasterGetPluginOffset(RwUInt32 pluginID);
RwBool       RwRasterValidatePlugins(const RwRaster * raster);

inline RwRaster *RwRasterSetFromImage(RwRaster *raster, RwImage *image) { using namespace rw; return raster->setFromImage(image); }




inline RwTexture *RwTextureCreate(RwRaster * raster) { using namespace rw; return Texture::create(raster); }
inline RwBool RwTextureDestroy(RwTexture * texture) { using namespace rw; texture->destroy(); return true; }
inline RwTexture *RwTextureAddRef(RwTexture *texture) { using namespace rw; texture->addRef(); return texture; }
// TODO
inline RwBool RwTextureSetMipmapping(RwBool enable) { using namespace rw; return true; }
RwBool RwTextureGetMipmapping(void);
// TODO
inline RwBool RwTextureSetAutoMipmapping(RwBool enable) { using namespace rw; return true; }
RwBool RwTextureGetAutoMipmapping(void);
RwBool RwTextureSetMipmapGenerationCallBack(RwTextureCallBackMipmapGeneration callback);
RwTextureCallBackMipmapGeneration RwTextureGetMipmapGenerationCallBack(void);
RwBool RwTextureSetMipmapNameCallBack(RwTextureCallBackMipmapName callback);
RwTextureCallBackMipmapName RwTextureGetMipmapNameCallBack(void);
RwBool RwTextureGenerateMipmapName(RwChar * name, RwChar * maskName, RwUInt8 mipLevel, RwInt32 format);
RwBool RwTextureRasterGenerateMipmaps(RwRaster * raster, RwImage * image);
RwTextureCallBackRead RwTextureGetReadCallBack(void);
RwBool RwTextureSetReadCallBack(RwTextureCallBackRead fpCallBack);
inline RwTexture *RwTextureSetName(RwTexture * texture, const RwChar * name) { using namespace rw; strncpy(texture->name, name, 32); return texture; }
RwTexture *RwTextureSetMaskName(RwTexture * texture, const RwChar * maskName);
inline RwChar *RwTextureGetName(RwTexture *texture) { using namespace rw; return texture->name; }
RwChar *RwTextureGetMaskName(RwTexture *texture);
inline RwTexture *RwTextureSetRaster(RwTexture * texture, RwRaster * raster) { using namespace rw; texture->raster = raster; return texture; }
inline RwTexture   *RwTextureRead(const RwChar * name, const RwChar * maskName) { using namespace rw; return Texture::read(name, maskName); }
inline RwRaster *RwTextureGetRaster(const RwTexture *texture) { using namespace rw; return texture->raster; }
RwInt32 RwTextureRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
RwInt32 RwTextureGetPluginOffset(RwUInt32 pluginID);
RwBool RwTextureValidatePlugins(const RwTexture * texture);

RwTexDictionary *RwTextureGetDictionary(RwTexture *texture);
inline RwTexture *RwTextureSetFilterMode(RwTexture *texture, RwTextureFilterMode filtering) { using namespace rw; texture->setFilter((Texture::FilterMode)filtering); return texture; }
RwTextureFilterMode RwTextureGetFilterMode(const RwTexture *texture);
inline RwTexture *RwTextureSetAddressing(RwTexture *texture, RwTextureAddressMode addressing) { using namespace rw;
	texture->setAddressU((Texture::Addressing)addressing);
	texture->setAddressV((Texture::Addressing)addressing);
	return texture;
}
inline RwTexture *RwTextureSetAddressingU(RwTexture *texture, RwTextureAddressMode addressing) { using namespace rw;
	texture->setAddressU((Texture::Addressing)addressing);
	return texture;
}
inline RwTexture *RwTextureSetAddressingV(RwTexture *texture, RwTextureAddressMode addressing) { using namespace rw;
	texture->setAddressV((Texture::Addressing)addressing);
	return texture;
}
RwTextureAddressMode RwTextureGetAddressing(const RwTexture *texture);
RwTextureAddressMode RwTextureGetAddressingU(const RwTexture *texture);
RwTextureAddressMode RwTextureGetAddressingV(const RwTexture *texture);

// TODO
inline void _rwD3D8TexDictionaryEnableRasterFormatConversion(bool enable) { using namespace rw; }


// revc: the native texture hack reader is gone: rw/TexRead.cpp reads natives (LIBRW_HOST)






inline RwTexDictionary *RwTexDictionaryCreate(void) { using namespace rw; return TexDictionary::create(); }
inline RwBool RwTexDictionaryDestroy(RwTexDictionary * dict) { using namespace rw; dict->destroy(); return true; }
inline RwTexture *RwTexDictionaryAddTexture(RwTexDictionary * dict, RwTexture * texture) { using namespace rw; dict->addFront(texture); return texture; }
//RwTexture *RwTexDictionaryRemoveTexture(RwTexture * texture);
inline RwTexture *RwTexDictionaryFindNamedTexture(RwTexDictionary * dict, const RwChar * name) { using namespace rw; return dict->find(name); }
inline RwTexDictionary *RwTexDictionaryGetCurrent(void) { using namespace rw; return TexDictionary::getCurrent(); }
inline RwTexDictionary *RwTexDictionarySetCurrent(RwTexDictionary * dict) { using namespace rw; TexDictionary::setCurrent(dict); return dict; }
inline const RwTexDictionary *RwTexDictionaryForAllTextures(const RwTexDictionary * dict, RwTextureCallBack fpCallBack, void *pData) { using namespace rw;
	FORLIST(lnk, ((RwTexDictionary*)dict)->textures)
		if(fpCallBack(Texture::fromDict(lnk), pData) == nil)
			break;
	return dict;
}
RwBool RwTexDictionaryForAllTexDictionaries(RwTexDictionaryCallBack fpCallBack, void *pData);
RwInt32 RwTexDictionaryRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
RwInt32 RwTexDictionaryGetPluginOffset(RwUInt32 pluginID);
RwBool RwTexDictionaryValidatePlugins(const RwTexDictionary * dict);
RwUInt32 RwTexDictionaryStreamGetSize(const RwTexDictionary *texDict);
RwTexDictionary *RwTexDictionaryStreamRead(RwStream *stream);
// revc: TXDs aren't written (the TXD image cache is only built when the video card can't do DXT,
// which the wgpu device always can); the caller handles the failure
inline const RwTexDictionary *RwTexDictionaryStreamWrite(const RwTexDictionary *texDict, RwStream *stream) { using namespace rw; return nil; }





inline RwStream *RwStreamOpen(RwStreamType type, RwStreamAccessType accessType, const void *pData) { using namespace rw;
	StreamFile *file;
	StreamMemory *mem;
	RwMemory *memargs;
	const char *mode;

	switch(accessType){
	case rwSTREAMREAD: mode = "rb"; break;
	case rwSTREAMWRITE: mode = "wb"; break;
	case rwSTREAMAPPEND: mode = "ab"; break;
	default: return nil;
	}

	// oh god this is horrible. librw streams really need fixing
	switch(type){
	case rwSTREAMFILENAME:{
		StreamFile fakefile;
		file = rwNewT(StreamFile, 1, 0);
		memcpy(file, &fakefile, sizeof(StreamFile));
#ifndef _WIN32
		char *r = casepath((char*)pData, true);
		if (r) {
			if (file->open((char*)r, mode)) {
				free(r);
				return file;
			}
			free(r);
		} else
#endif
		{
			if (file->open((char*)pData, mode))
				return file;
		}
		rwFree(file);
		return nil;
	}
	case rwSTREAMMEMORY:{
		StreamMemory fakemem;
		memargs = (RwMemory*)pData;
		mem = rwNewT(StreamMemory, 1, 0);
		memcpy(mem, &fakemem, sizeof(StreamMemory));
		if(mem->open(memargs->start, memargs->length))
			return mem;
		rwFree(mem);
		return nil;
	}
	default:
		assert(0 && "unknown type");
		return nil;
	}
}
inline RwBool RwStreamClose(RwStream * stream, void *pData) { using namespace rw; stream->close(); rwFree(stream); return true; }
inline RwUInt32 RwStreamRead(RwStream * stream, void *buffer, RwUInt32 length) { using namespace rw; return stream->read8(buffer, length); }
inline RwStream *RwStreamWrite(RwStream * stream, const void *buffer, RwUInt32 length) { using namespace rw; stream->write8(buffer, length); return stream; }
inline RwStream *RwStreamSkip(RwStream * stream, RwUInt32 offset) { using namespace rw; stream->seek(offset); return stream; }

inline RwBool RwStreamFindChunk(RwStream *stream, RwUInt32 type, RwUInt32 *lengthOut, RwUInt32 *versionOut)
	{ using namespace rw; return findChunk(stream, type, lengthOut, versionOut); }



inline void RwIm2DVertexSetCameraX(RwIm2DVertex *vert, RwReal camx) { using namespace rw; }
inline void RwIm2DVertexSetCameraY(RwIm2DVertex *vert, RwReal camy) { using namespace rw; }
inline void RwIm2DVertexSetCameraZ(RwIm2DVertex *vert, RwReal camz) { using namespace rw; vert->setCameraZ(camz); }
inline void RwIm2DVertexSetRecipCameraZ(RwIm2DVertex *vert, RwReal recipz) { using namespace rw; vert->setRecipCameraZ(recipz); }
inline void RwIm2DVertexSetScreenX(RwIm2DVertex *vert, RwReal scrnx) { using namespace rw; vert->setScreenX(scrnx); }
inline void RwIm2DVertexSetScreenY(RwIm2DVertex *vert, RwReal scrny) { using namespace rw; vert->setScreenY(scrny); }
inline void RwIm2DVertexSetScreenZ(RwIm2DVertex *vert, RwReal scrnz) { using namespace rw; vert->setScreenZ(scrnz); }
inline void RwIm2DVertexSetU(RwIm2DVertex *vert, RwReal texU, RwReal recipz) { using namespace rw; vert->setU(texU, recipz); }
inline void RwIm2DVertexSetV(RwIm2DVertex *vert, RwReal texV, RwReal recipz) { using namespace rw; vert->setV(texV, recipz); }
inline void RwIm2DVertexSetIntRGBA(RwIm2DVertex *vert, RwUInt8 red, RwUInt8 green, RwUInt8 blue, RwUInt8 alpha) { using namespace rw; vert->setColor(red, green, blue, alpha); }

inline RwReal RwIm2DGetNearScreenZ(void) { using namespace rw; return im2d::GetNearZ(); }
inline RwReal RwIm2DGetFarScreenZ(void) { using namespace rw; return im2d::GetFarZ(); }
inline RwBool RwIm2DRenderLine(RwIm2DVertex *vertices, RwInt32 numVertices, RwInt32 vert1, RwInt32 vert2)
	{ using namespace rw; im2d::RenderLine(vertices, numVertices, vert1, vert2); return true; }
inline RwBool RwIm2DRenderTriangle(RwIm2DVertex *vertices, RwInt32 numVertices, RwInt32 vert1, RwInt32 vert2, RwInt32 vert3 )
	{ using namespace rw; im2d::RenderTriangle(vertices, numVertices, vert1, vert2, vert3); return true; }
inline RwBool RwIm2DRenderPrimitive(RwPrimitiveType primType, RwIm2DVertex *vertices, RwInt32 numVertices)
	{ using namespace rw; im2d::RenderPrimitive((PrimitiveType)primType, vertices, numVertices); return true; }
inline RwBool RwIm2DRenderIndexedPrimitive(RwPrimitiveType primType, RwIm2DVertex *vertices, RwInt32 numVertices, RwImVertexIndex *indices, RwInt32 numIndices)
	{ using namespace rw; im2d::RenderIndexedPrimitive((PrimitiveType)primType, vertices, numVertices, indices, numIndices); return true; }


inline void RwIm3DVertexSetPos(RwIm3DVertex *vert, RwReal x, RwReal y, RwReal z) { using namespace rw; vert->setX(x); vert->setY(y); vert->setZ(z); }
inline void RwIm3DVertexSetU(RwIm3DVertex *vert, RwReal u) { using namespace rw; vert->setU(u); }
inline void RwIm3DVertexSetV(RwIm3DVertex *vert, RwReal v) { using namespace rw; vert->setV(v); }
inline void RwIm3DVertexSetRGBA(RwIm3DVertex *vert, RwUInt8 r, RwUInt8 g, RwUInt8 b, RwUInt8 a) { using namespace rw; vert->setColor(r, g, b, a); }

inline void  *RwIm3DTransform(RwIm3DVertex *pVerts, RwUInt32 numVerts, RwMatrix *ltm, RwUInt32 flags) { using namespace rw; im3d::Transform(pVerts, numVerts, ltm, flags); return pVerts; }
inline RwBool RwIm3DEnd(void) { using namespace rw; im3d::End(); return true; }
inline RwBool RwIm3DRenderLine(RwInt32 vert1, RwInt32 vert2) { using namespace rw;
	RwImVertexIndex indices[2];
	indices[0] = vert1;
	indices[1] = vert2;
	im3d::RenderIndexedPrimitive((PrimitiveType)PRIMTYPELINELIST, indices, 2);
	return true;
}
RwBool RwIm3DRenderTriangle(RwInt32 vert1, RwInt32 vert2, RwInt32 vert3);
inline RwBool RwIm3DRenderIndexedPrimitive(RwPrimitiveType primType, RwImVertexIndex *indices, RwInt32 numIndices) { using namespace rw; im3d::RenderIndexedPrimitive((PrimitiveType)primType, indices, numIndices); return true; }
RwBool RwIm3DRenderPrimitive(RwPrimitiveType primType);





inline RwBool RwRenderStateGet(RwRenderState state, void *value)
{ using namespace rw;
	uint32 *uival = (uint32*)value;
	uint32 fog;
	switch(state){
	case rwRENDERSTATETEXTURERASTER: *(void**)value = GetRenderStatePtr(TEXTURERASTER); return true;
	case rwRENDERSTATETEXTUREADDRESS: *uival = GetRenderState(TEXTUREADDRESS); return true;
	case rwRENDERSTATETEXTUREADDRESSU: *uival = GetRenderState(TEXTUREADDRESSU); return true;
	case rwRENDERSTATETEXTUREADDRESSV: *uival = GetRenderState(TEXTUREADDRESSV); return true;
	case rwRENDERSTATETEXTUREPERSPECTIVE: *uival = 1; return true;
	case rwRENDERSTATEZTESTENABLE: *uival = GetRenderState(ZTESTENABLE); return true;
	case rwRENDERSTATESHADEMODE: *uival = rwSHADEMODEGOURAUD; return true;
	case rwRENDERSTATEZWRITEENABLE: *uival = GetRenderState(ZWRITEENABLE); return true;
	case rwRENDERSTATETEXTUREFILTER: *uival = GetRenderState(TEXTUREFILTER); return true;
	case rwRENDERSTATESRCBLEND: *uival = GetRenderState(SRCBLEND); return true;
	case rwRENDERSTATEDESTBLEND: *uival = GetRenderState(DESTBLEND); return true;
	case rwRENDERSTATEVERTEXALPHAENABLE: *uival = GetRenderState(VERTEXALPHA); return true;
	case rwRENDERSTATEBORDERCOLOR: *uival = 0; return true;
	case rwRENDERSTATEFOGENABLE: *uival = GetRenderState(FOGENABLE); return true;
	case rwRENDERSTATEFOGCOLOR:
		// have to swap R and B here
		fog = GetRenderState(FOGCOLOR);
		*uival = (fog>>16)&0xFF;
		*uival |= (fog&0xFF)<<16;
		*uival |= fog&0xFF00;
		*uival |= fog&0xFF000000;
		return true;
	case rwRENDERSTATEFOGTYPE: *uival = rwFOGTYPELINEAR; return true;
	case rwRENDERSTATEFOGDENSITY: *(float*)value = 1.0f; return true;
	case rwRENDERSTATECULLMODE: *uival = GetRenderState(CULLMODE); return true;

	// all unsupported
	case rwRENDERSTATEFOGTABLE:
	case rwRENDERSTATEALPHAPRIMITIVEBUFFER:

	case rwRENDERSTATESTENCILENABLE:
	case rwRENDERSTATESTENCILFAIL:
	case rwRENDERSTATESTENCILZFAIL:
	case rwRENDERSTATESTENCILPASS:
	case rwRENDERSTATESTENCILFUNCTION:
	case rwRENDERSTATESTENCILFUNCTIONREF:
	case rwRENDERSTATESTENCILFUNCTIONMASK:
	case rwRENDERSTATESTENCILFUNCTIONWRITEMASK:
	default:
		return false;
	}
}
inline RwBool RwRenderStateSet(RwRenderState state, void *value)
{ using namespace rw;
	uint32 uival = (uintptr)value;
	uint32 fog;
	switch(state){
	case rwRENDERSTATETEXTURERASTER: SetRenderStatePtr(TEXTURERASTER, value); return true;
	case rwRENDERSTATETEXTUREADDRESS: SetRenderState(TEXTUREADDRESS, uival); return true;
	case rwRENDERSTATETEXTUREADDRESSU: SetRenderState(TEXTUREADDRESSU, uival); return true;
	case rwRENDERSTATETEXTUREADDRESSV: SetRenderState(TEXTUREADDRESSV, uival); return true;
	case rwRENDERSTATETEXTUREPERSPECTIVE: return true;
	case rwRENDERSTATEZTESTENABLE: SetRenderState(ZTESTENABLE, uival); return true;
	case rwRENDERSTATESHADEMODE: return true;
	case rwRENDERSTATEZWRITEENABLE: SetRenderState(ZWRITEENABLE, uival); return true;
	case rwRENDERSTATETEXTUREFILTER: SetRenderState(TEXTUREFILTER, uival); return true;
	case rwRENDERSTATESRCBLEND: SetRenderState(SRCBLEND, uival); return true;
	case rwRENDERSTATEDESTBLEND: SetRenderState(DESTBLEND, uival); return true;
	case rwRENDERSTATEVERTEXALPHAENABLE: SetRenderState(VERTEXALPHA, uival); return true;
	case rwRENDERSTATEBORDERCOLOR: return true;
	case rwRENDERSTATEFOGENABLE: SetRenderState(FOGENABLE, uival); return true;
	case rwRENDERSTATEFOGCOLOR:
		// have to swap R and B here
		fog = (uival>>16)&0xFF;
		fog |= (uival&0xFF)<<16;
		fog |= uival&0xFF00;
		fog |= uival&0xFF000000;
		SetRenderState(FOGCOLOR, fog);
		return true;
	case rwRENDERSTATEFOGTYPE: return true;
	case rwRENDERSTATEFOGDENSITY: return true;
	case rwRENDERSTATEFOGTABLE: return true;
	case rwRENDERSTATEALPHAPRIMITIVEBUFFER: return true;
	case rwRENDERSTATECULLMODE: SetRenderState(CULLMODE, uival); return true;

	// all unsupported
	case rwRENDERSTATESTENCILENABLE:
	case rwRENDERSTATESTENCILFAIL:
	case rwRENDERSTATESTENCILZFAIL:
	case rwRENDERSTATESTENCILPASS:
	case rwRENDERSTATESTENCILFUNCTION:
	case rwRENDERSTATESTENCILFUNCTIONREF:
	case rwRENDERSTATESTENCILFUNCTIONMASK:
	case rwRENDERSTATESTENCILFUNCTIONWRITEMASK:
	default:
		return true;
	}
}

inline rw::MemoryFunctions gMemfuncs;
inline void *(*real_malloc)(size_t size);
inline void *(*real_realloc)(void *mem, size_t newSize);
inline void *mallocWrap(size_t sz, rw::uint32 hint) { if(sz == 0) return nil; return real_malloc(sz); }
inline void *reallocWrap(void *p, size_t sz, rw::uint32 hint) { return real_realloc(p, sz); }


// WARNING: unused parameters
inline RwBool RwEngineInit(RwMemoryFunctions *memFuncs, RwUInt32 initFlags, RwUInt32 resArenaSize) { using namespace rw;
	if(memFuncs){
		real_malloc = memFuncs->rwmalloc;
		real_realloc = memFuncs->rwrealloc;
		gMemfuncs.rwmalloc = mallocWrap;
		gMemfuncs.rwrealloc = reallocWrap;
		gMemfuncs.rwfree = memFuncs->rwfree;
		Engine::init(&gMemfuncs);
	}else{
		Engine::init(nil);
	}
	return true;
}
// TODO: this is platform dependent
inline RwBool RwEngineOpen(RwEngineOpenParams *initParams) { using namespace rw;
	static EngineOpenParams openParams;
#ifdef RW_D3D9
	openParams.window = (HWND)initParams->displayID;
#else
	openParams = *(EngineOpenParams*)initParams->displayID;
#endif
	return Engine::open(&openParams);
}
inline RwBool RwEngineStart(void) { using namespace rw;
	return Engine::start();
}
inline RwBool RwEngineStop(void) { using namespace rw; Engine::stop(); return true; }
inline RwBool RwEngineClose(void) { using namespace rw; Engine::close(); return true; }
inline RwBool RwEngineTerm(void) { using namespace rw; Engine::term(); return true; }
RwInt32 RwEngineRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor initCB, RwPluginObjectDestructor termCB);
RwInt32 RwEngineGetPluginOffset(RwUInt32 pluginID);
inline RwInt32 RwEngineGetNumSubSystems(void) { using namespace rw; return Engine::getNumSubSystems(); }
inline RwSubSystemInfo *RwEngineGetSubSystemInfo(RwSubSystemInfo *subSystemInfo, RwInt32 subSystemIndex)
	{ using namespace rw; return Engine::getSubSystemInfo(subSystemInfo, subSystemIndex); }
inline RwInt32 RwEngineGetCurrentSubSystem(void) { using namespace rw; return Engine::getCurrentSubSystem(); }
inline RwBool RwEngineSetSubSystem(RwInt32 subSystemIndex) { using namespace rw; return Engine::setSubSystem(subSystemIndex); }
inline RwInt32 RwEngineGetNumVideoModes(void) { using namespace rw; return Engine::getNumVideoModes(); }
inline RwVideoMode *RwEngineGetVideoModeInfo(RwVideoMode *modeinfo, RwInt32 modeIndex)
	{ using namespace rw; return Engine::getVideoModeInfo(modeinfo, modeIndex); }
inline RwInt32 RwEngineGetCurrentVideoMode(void) { using namespace rw; return Engine::getCurrentVideoMode(); }
inline RwBool RwEngineSetVideoMode(RwInt32 modeIndex) { using namespace rw; return Engine::setVideoMode(modeIndex); }
RwInt32 RwEngineGetTextureMemorySize(void);
RwInt32 RwEngineGetMaxTextureSize(void);



// TODO
inline void RwD3D8EngineSetRefreshRate(RwUInt32 refreshRate) { using namespace rw;}
inline RwBool RwD3D8DeviceSupportsDXTTexture(void) { using namespace rw; return true; }


inline void RwD3D8EngineSetMultiSamplingLevels(RwUInt32 level) { using namespace rw; Engine::setMultiSamplingLevels(level); }
inline RwUInt32 RwD3D8EngineGetMaxMultiSamplingLevels(void) { using namespace rw; return Engine::getMaxMultiSamplingLevels(); }


inline RpMaterial *RpMaterialCreate(void) { using namespace rw; return Material::create(); }
inline RwBool RpMaterialDestroy(RpMaterial *material) { using namespace rw; material->destroy(); return true; }
//RpMaterial *RpMaterialClone(RpMaterial *material);
inline RpMaterial *RpMaterialSetTexture(RpMaterial *material, RwTexture *texture) { using namespace rw; material->setTexture(texture); return material; }
//RpMaterial *RpMaterialAddRef(RpMaterial *material);
inline RwTexture *RpMaterialGetTexture(const RpMaterial *material) { using namespace rw; return material->texture; }
inline RpMaterial *RpMaterialSetColor(RpMaterial *material, const RwRGBA *color) { using namespace rw; material->color = *color; return material; }
inline const RwRGBA *RpMaterialGetColor(const RpMaterial *material) { using namespace rw; return &material->color; }
RpMaterial *RpMaterialSetSurfaceProperties(RpMaterial *material, const RwSurfaceProperties *surfaceProperties);
inline const RwSurfaceProperties *RpMaterialGetSurfaceProperties(const RpMaterial *material) { using namespace rw; return &material->surfaceProps; }
//RwInt32 RpMaterialRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
//RwInt32 RpMaterialRegisterPluginStream(RwUInt32 pluginID, RwPluginDataChunkReadCallBack readCB, RwPluginDataChunkWriteCallBack writeCB, RwPluginDataChunkGetSizeCallBack getSizeCB);
//RwInt32 RpMaterialSetStreamAlwaysCallBack(RwUInt32 pluginID, RwPluginDataChunkAlwaysCallBack alwaysCB);
//RwInt32 RpMaterialGetPluginOffset(RwUInt32 pluginID);
//RwBool RpMaterialValidatePlugins(const RpMaterial *material);
//RwUInt32 RpMaterialStreamGetSize(const RpMaterial *material);
//RpMaterial *RpMaterialStreamRead(RwStream *stream);
//const RpMaterial *RpMaterialStreamWrite(const RpMaterial *material, RwStream *stream);
//RpMaterialChunkInfo *_rpMaterialChunkInfoRead(RwStream *stream,  RpMaterialChunkInfo *materialChunkInfo, RwInt32 *bytesRead);





inline RwReal RpLightGetRadius(const RpLight *light) { using namespace rw; return light->radius; }
//const RwRGBAReal *RpLightGetColor(const RpLight *light);
inline RpLight *RpLightSetFrame(RpLight *light, RwFrame *frame) { using namespace rw; light->setFrame(frame); return light; }
inline RwFrame *RpLightGetFrame(const RpLight *light) { using namespace rw; return light->getFrame(); }
//RpLightType RpLightGetType(const RpLight *light);
inline RpLight *RpLightSetFlags(RpLight *light, RwUInt32 flags) { using namespace rw; light->setFlags(flags); return light; }
//RwUInt32 RpLightGetFlags(const RpLight *light);
inline RpLight *RpLightCreate(RwInt32 type) { using namespace rw; return rw::Light::create(type); }
inline RwBool RpLightDestroy(RpLight *light) { using namespace rw; light->destroy(); return true; }
inline RpLight *RpLightSetRadius(RpLight *light, RwReal radius) { using namespace rw; light->radius = radius; return light; }
inline RpLight *RpLightSetColor(RpLight *light, const RwRGBAReal *color) { using namespace rw; light->setColor(color->red, color->green, color->blue); return light; }
//RwReal RpLightGetConeAngle(const RpLight *light);
//RpLight *RpLightSetConeAngle(RpLight * ight, RwReal angle);
//RwUInt32 RpLightStreamGetSize(const RpLight *light);
//RpLight *RpLightStreamRead(RwStream *stream);
//const RpLight *RpLightStreamWrite(const RpLight *light, RwStream *stream);
//RpLightChunkInfo *_rpLightChunkInfoRead(RwStream *stream, RpLightChunkInfo *lightChunkInfo, RwInt32 *bytesRead);
//RwInt32 RpLightRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
//RwInt32 RpLightRegisterPluginStream(RwUInt32 pluginID, RwPluginDataChunkReadCallBack readCB, RwPluginDataChunkWriteCallBack writeCB, RwPluginDataChunkGetSizeCallBack getSizeCB);
//RwInt32 RpLightSetStreamAlwaysCallBack(RwUInt32 pluginID, RwPluginDataChunkAlwaysCallBack alwaysCB);
//RwInt32 RpLightGetPluginOffset(RwUInt32 pluginID);
//RwBool RpLightValidatePlugins(const RpLight * light);





inline RpGeometry  *RpGeometryCreate(RwInt32 numVert, RwInt32 numTriangles, RwUInt32 format) { using namespace rw; return Geometry::create(numVert, numTriangles, format); }
inline RwBool RpGeometryDestroy(RpGeometry *geometry) { using namespace rw; geometry->destroy(); return true; }
RpGeometry *_rpGeometryAddRef(RpGeometry *geometry);
inline RpGeometry  *RpGeometryLock(RpGeometry *geometry, RwInt32 lockMode) { using namespace rw; geometry->lock(lockMode); return geometry; }
inline RpGeometry  *RpGeometryUnlock(RpGeometry *geometry) { using namespace rw; geometry->unlock(); return geometry; }
RpGeometry  *RpGeometryTransform(RpGeometry *geometry, const RwMatrix *matrix);
RpGeometry  *RpGeometryCreateSpace(RwReal radius);
inline RpMorphTarget  *RpMorphTargetSetBoundingSphere(RpMorphTarget *morphTarget, const RwSphere *boundingSphere) { using namespace rw; morphTarget->boundingSphere = *boundingSphere; return morphTarget; }
inline RwSphere  *RpMorphTargetGetBoundingSphere(RpMorphTarget *morphTarget) { using namespace rw; return &morphTarget->boundingSphere; }
inline const RpMorphTarget  *RpMorphTargetCalcBoundingSphere(const RpMorphTarget *morphTarget, RwSphere *boundingSphere) { using namespace rw; *boundingSphere = morphTarget->calculateBoundingSphere(); return morphTarget; }
inline RwInt32 RpGeometryAddMorphTargets(RpGeometry *geometry, RwInt32 mtcount) { using namespace rw; RwInt32 n = geometry->numMorphTargets; geometry->addMorphTargets(mtcount); return n; }
inline RwInt32 RpGeometryAddMorphTarget(RpGeometry *geometry) { using namespace rw; return RpGeometryAddMorphTargets(geometry, 1); }
RpGeometry  *RpGeometryRemoveMorphTarget(RpGeometry *geometry, RwInt32 morphTarget);
RwInt32 RpGeometryGetNumMorphTargets(const RpGeometry *geometry);
inline RpMorphTarget  *RpGeometryGetMorphTarget(const RpGeometry *geometry, RwInt32 morphTarget) { using namespace rw; return &geometry->morphTargets[morphTarget]; }
inline RwRGBA  *RpGeometryGetPreLightColors(const RpGeometry *geometry) { using namespace rw; return geometry->colors; }
inline RwTexCoords  *RpGeometryGetVertexTexCoords(const RpGeometry *geometry, RwTextureCoordinateIndex uvIndex) { using namespace rw;
	if(uvIndex == rwNARWTEXTURECOORDINATEINDEX)
		return nil;
	return geometry->texCoords[uvIndex-rwTEXTURECOORDINATEINDEX0];
}
inline RwInt32 RpGeometryGetNumTexCoordSets(const RpGeometry *geometry) { using namespace rw; return geometry->numTexCoordSets; }
inline RwInt32 RpGeometryGetNumVertices (const RpGeometry *geometry) { using namespace rw; return geometry->numVertices; }
inline RwV3d  *RpMorphTargetGetVertices(const RpMorphTarget *morphTarget) { using namespace rw; return morphTarget->vertices; }
inline RwV3d  *RpMorphTargetGetVertexNormals(const RpMorphTarget *morphTarget) { using namespace rw; return morphTarget->normals; }
inline RpTriangle  *RpGeometryGetTriangles(const RpGeometry *geometry) { using namespace rw; return geometry->triangles; }
inline RwInt32 RpGeometryGetNumTriangles(const RpGeometry *geometry) { using namespace rw; return geometry->numTriangles; }
inline RpMaterial  *RpGeometryGetMaterial(const RpGeometry *geometry, RwInt32 matNum) { using namespace rw; return geometry->matList.materials[matNum]; }
inline const RpGeometry  *RpGeometryTriangleSetVertexIndices(const RpGeometry *geometry, RpTriangle *triangle, RwUInt16 vert1, RwUInt16 vert2, RwUInt16 vert3)
	{ using namespace rw; triangle->v[0] = vert1; triangle->v[1] = vert2; triangle->v[2] = vert3; return geometry; }
inline RpGeometry  *RpGeometryTriangleSetMaterial(RpGeometry *geometry, RpTriangle *triangle, RpMaterial *material) { using namespace rw;
	int id = geometry->matList.findIndex(material);
	if(id < 0)
		id = geometry->matList.appendMaterial(material);
	if(id < 0)
		return nil;
	triangle->matId = id;
	return geometry;
}
const RpGeometry  *RpGeometryTriangleGetVertexIndices(const RpGeometry *geometry, const RpTriangle *triangle, RwUInt16 *vert1, RwUInt16 *vert2, RwUInt16 *vert3);
RpMaterial   *RpGeometryTriangleGetMaterial(const RpGeometry *geometry, const RpTriangle *triangle);
RwInt32 RpGeometryGetNumMaterials(const RpGeometry *geometry);
inline RpGeometry  *RpGeometryForAllMaterials(RpGeometry *geometry, RpMaterialCallBack fpCallBack, void *pData) { using namespace rw;
	int i;
	for(i = 0; i < geometry->matList.numMaterials; i++)
		if(fpCallBack(geometry->matList.materials[i], pData) == nil)
			break;
	return geometry;
}
//const RpGeometry  *RpGeometryForAllMeshes(const RpGeometry *geometry, RpMeshCallBack fpCallBack, void *pData);
RwInt32 RpGeometryRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB);
RwInt32 RpGeometryRegisterPluginStream(RwUInt32 pluginID, RwPluginDataChunkReadCallBack readCB, RwPluginDataChunkWriteCallBack writeCB, RwPluginDataChunkGetSizeCallBack getSizeCB);
RwInt32 RpGeometrySetStreamAlwaysCallBack(RwUInt32 pluginID, RwPluginDataChunkAlwaysCallBack alwaysCB);
RwInt32 RpGeometryGetPluginOffset(RwUInt32 pluginID);
RwBool RpGeometryValidatePlugins(const RpGeometry *geometry);
RwUInt32 RpGeometryStreamGetSize(const RpGeometry *geometry);
const RpGeometry  *RpGeometryStreamWrite(const RpGeometry *geometry, RwStream *stream);
// RpGeometryStreamRead: models are read in Rust (rw/DffRead.cpp)
//RpGeometryChunkInfo *_rpGeometryChunkInfoRead(RwStream *stream, RpGeometryChunkInfo *geometryChunkInfo, RwInt32 *bytesRead);
inline RwUInt32 RpGeometryGetFlags(const RpGeometry *geometry) { using namespace rw; return geometry->flags; }
inline RpGeometry  *RpGeometrySetFlags(RpGeometry *geometry, RwUInt32 flags) { using namespace rw; geometry->flags = flags; return geometry; }
const RwSurfaceProperties *_rpGeometryGetSurfaceProperties(const RpGeometry *geometry);
RpGeometry *_rpGeometrySetSurfaceProperties(RpGeometry *geometry, const RwSurfaceProperties *surfaceProperties);





inline RwFrame *RpClumpGetFrame(const RpClump * clump) { using namespace rw; return clump->getFrame(); }
inline RpClump *RpClumpSetFrame(RpClump * clump, RwFrame * frame) { using namespace rw; clump->setFrame(frame); return clump; }
inline RpClump *RpClumpForAllAtomics(RpClump * clump, RpAtomicCallBack callback, void *pData) { using namespace rw;
	FORLIST(lnk, clump->atomics)
		if(callback(Atomic::fromClump(lnk), pData) == nil)
			break;
	return clump;
}
RpClump *RpClumpForAllLights(RpClump * clump, RpLightCallBack callback, void *pData);
RpClump *RpClumpForAllCameras(RpClump * clump, RwCameraCallBack callback, void *pData);
//RpClump *RpClumpCreateSpace(const RwV3d * position, RwReal radius);
inline RpClump *RpClumpRender(RpClump * clump) { using namespace rw; clump->render(); return clump; }
inline RpClump *RpClumpRemoveAtomic(RpClump * clump, RpAtomic * atomic) { using namespace rw; clump->removeAtomic(atomic); return clump; }
inline RpClump *RpClumpAddAtomic(RpClump * clump, RpAtomic * atomic) { using namespace rw; clump->addAtomic(atomic); return clump; }
//RpClump *RpClumpRemoveLight(RpClump * clump, RpLight * light);
//RpClump *RpClumpAddLight(RpClump * clump, RpLight * light);
//RpClump *RpClumpRemoveCamera(RpClump * clump, RwCamera * camera);
//RpClump *RpClumpAddCamera(RpClump * clump, RwCamera * camera);
inline RwBool RpClumpDestroy(RpClump * clump) { using namespace rw; clump->destroy(); return true; }
inline RpClump *RpClumpCreate(void) { using namespace rw; return rw::Clump::create(); }
inline RpClump *RpClumpClone(RpClump * clump) { using namespace rw; return clump->clone(); }
//RpClump *RpClumpSetCallBack(RpClump * clump, RpClumpCallBack callback);
//RpClumpCallBack RpClumpGetCallBack(const RpClump * clump);
inline RwInt32 RpClumpGetNumAtomics(RpClump * clump) { using namespace rw; return clump->countAtomics(); }
//RwInt32 RpClumpGetNumLights(RpClump * clump);
//RwInt32 RpClumpGetNumCameras(RpClump * clump);
// revc: models are read in Rust (rw/DffRead.cpp, stage 2 step 2); librw's readers are gone.
rw::Clump *RevcClumpStreamRead(RwStream *stream);
inline RpClump *RpClumpStreamRead(RwStream * stream) { using namespace rw; return RevcClumpStreamRead(stream); }
//RpClump *RpClumpStreamWrite(RpClump * clump, RwStream * stream);
inline RwInt32 RpClumpRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB)
	{ using namespace rw; return Clump::registerPlugin(size, pluginID, constructCB, destructCB, (CopyConstructor)copyCB); }
inline RwInt32 RpClumpRegisterPluginStream(RwUInt32 pluginID, RwPluginDataChunkReadCallBack  readCB, RwPluginDataChunkWriteCallBack writeCB, RwPluginDataChunkGetSizeCallBack getSizeCB)
	{ using namespace rw; return Clump::registerPluginStream(pluginID, readCB, (StreamWrite)writeCB, (StreamGetSize)getSizeCB); }
//RwInt32 RpClumpSetStreamAlwaysCallBack(RwUInt32 pluginID, RwPluginDataChunkAlwaysCallBack alwaysCB);
//RwInt32 RpClumpGetPluginOffset(RwUInt32 pluginID);
//RwBool RpClumpValidatePlugins(const RpClump * clump);



inline RpAtomic *RpAtomicCreate(void) { using namespace rw; return rw::Atomic::create(); }
inline RwBool RpAtomicDestroy(RpAtomic * atomic) { using namespace rw; atomic->destroy(); return true; }
inline RpAtomic *RpAtomicClone(RpAtomic * atomic) { using namespace rw; return atomic->clone(); }
inline RpAtomic *RpAtomicSetFrame(RpAtomic * atomic, RwFrame * frame) { using namespace rw; atomic->setFrame(frame); return atomic; }
inline RpAtomic *RpAtomicSetGeometry(RpAtomic * atomic, RpGeometry * geometry, RwUInt32 flags) { using namespace rw; atomic->setGeometry(geometry, flags); return atomic; }

inline RwFrame *RpAtomicGetFrame(const RpAtomic * atomic) { using namespace rw; return atomic->getFrame(); }
inline RpAtomic *RpAtomicSetFlags(RpAtomic * atomic, RwUInt32 flags) { using namespace rw; atomic->setFlags(flags); return atomic; }
inline RwUInt32 RpAtomicGetFlags(const RpAtomic * atomic) { using namespace rw; return atomic->getFlags(); }
inline RwSphere *RpAtomicGetBoundingSphere(RpAtomic * atomic) { using namespace rw; return &atomic->boundingSphere; }
inline RpAtomic *RpAtomicRender(RpAtomic * atomic) { using namespace rw; atomic->render(); return atomic; }
inline RpClump *RpAtomicGetClump(const RpAtomic * atomic) { using namespace rw; return atomic->clump; }
//RpInterpolator *RpAtomicGetInterpolator(RpAtomic * atomic);
inline RpGeometry *RpAtomicGetGeometry(const RpAtomic * atomic) { using namespace rw; return atomic->geometry; }
// WARNING: illegal cast
inline void RpAtomicSetRenderCallBack(RpAtomic * atomic, RpAtomicCallBackRender callback) { using namespace rw; atomic->setRenderCB((Atomic::RenderCB)callback); }
inline RpAtomicCallBackRender RpAtomicGetRenderCallBack(const RpAtomic * atomic) { using namespace rw; return (RpAtomicCallBackRender)atomic->renderCB; }
//RwBool RpAtomicInstance(RpAtomic *atomic);
//RwUInt32 RpAtomicStreamGetSize(RpAtomic * atomic);
//RpAtomic *RpAtomicStreamRead(RwStream * stream);
//RpAtomic *RpAtomicStreamWrite(RpAtomic * atomic, RwStream * stream);
inline RwInt32 RpAtomicRegisterPlugin(RwInt32 size, RwUInt32 pluginID, RwPluginObjectConstructor constructCB, RwPluginObjectDestructor destructCB, RwPluginObjectCopy copyCB)
	{ using namespace rw; return Atomic::registerPlugin(size, pluginID, constructCB, destructCB, (CopyConstructor)copyCB); }
//RwInt32 RpAtomicRegisterPluginStream(RwUInt32 pluginID, RwPluginDataChunkReadCallBack readCB, RwPluginDataChunkWriteCallBack writeCB, RwPluginDataChunkGetSizeCallBack getSizeCB);
//RwInt32 RpAtomicSetStreamAlwaysCallBack(RwUInt32 pluginID, RwPluginDataChunkAlwaysCallBack alwaysCB);
//RwInt32 RpAtomicSetStreamRightsCallBack(RwUInt32 pluginID, RwPluginDataChunkRightsCallBack rightsCB);
//RwInt32 RpAtomicGetPluginOffset(RwUInt32 pluginID);
//RwBool RpAtomicValidatePlugins(const RpAtomic * atomic);

inline RpAtomic *AtomicDefaultRenderCallBack(RpAtomic * atomic) { using namespace rw; Atomic::defaultRenderCB(atomic); return atomic; }


// TODO: this is extremely simplified
inline RpWorld     *RpWorldCreate(RwBBox * boundingBox) { using namespace rw; return World::create(); }
inline RwBool       RpWorldDestroy(RpWorld * world) { using namespace rw; world->destroy(); return true; }

inline RwBool       RpWorldPluginAttach(void) { using namespace rw;
	registerMeshPlugin();
	registerNativeDataPlugin();
	registerAtomicRightsPlugin();
	registerMaterialRightsPlugin();

	return true;
}

inline RpWorld *RpWorldRemoveCamera(RpWorld *world, RwCamera *camera) { using namespace rw; world->removeCamera(camera); return world; }
inline RpWorld *RpWorldAddCamera(RpWorld *world, RwCamera *camera) { using namespace rw; world->addCamera(camera); return world; }
RpWorld *RwCameraGetWorld(const RwCamera *camera);
RpWorld *RpWorldRemoveAtomic(RpWorld *world, RpAtomic *atomic);
RpWorld *RpWorldAddAtomic(RpWorld *world, RpAtomic *atomic);
RpWorld *RpAtomicGetWorld(const RpAtomic *atomic);
RpWorld *RpWorldAddClump(RpWorld *world, RpClump *clump);
RpWorld *RpWorldRemoveClump(RpWorld *world, RpClump *clump);
RpWorld *RpClumpGetWorld(const RpClump *clump);
inline RpWorld *RpWorldAddLight(RpWorld *world, RpLight *light) { using namespace rw; world->addLight(light); return world; }
inline RpWorld *RpWorldRemoveLight(RpWorld *world, RpLight *light) { using namespace rw; world->removeLight(light); return world; }
RpWorld *RpLightGetWorld(const RpLight *light);
RwCamera *RwCameraForAllClumpsInFrustum(RwCamera *camera, void *data);
RwCamera *RwCameraForAllClumpsNotInFrustum(RwCamera *camera, RwInt32 numClumps, void *data);




inline RwBool RpMatFXPluginAttach( void ) { using namespace rw; registerMatFXPlugin(); return true; }
inline RpAtomic *RpMatFXAtomicEnableEffects( RpAtomic *atomic ) { using namespace rw; MatFX::enableEffects(atomic); return atomic; }
inline RpMatFXMaterialFlags RpMatFXMaterialGetEffects( const RpMaterial *material ){ using namespace rw; return (RpMatFXMaterialFlags)MatFX::getEffects(material); }
inline RpMaterial *RpMatFXMaterialSetEffects( RpMaterial *material, RpMatFXMaterialFlags flags ) { using namespace rw; MatFX::setEffects(material, (uint32)flags); return material; }
inline RpMaterial *RpMatFXMaterialSetupEnvMap( RpMaterial *material, RwTexture *texture, RwFrame *frame, RwBool useFrameBufferAlpha, RwReal coef ) { using namespace rw;
	MatFX *mfx = MatFX::get(material);
	mfx->setEnvTexture(texture);
	mfx->setEnvFrame(frame);
	mfx->setEnvCoefficient(coef);
	return material;
}
inline RpMaterial *RpMatFXMaterialSetEnvMapFrame( RpMaterial *material, RwFrame *frame )
{ using namespace rw;
	MatFX *mfx = MatFX::get(material);
	mfx->setEnvFrame(frame);
	return material;
}
inline RpMaterial *RpMatFXMaterialSetEnvMapFrameBufferAlpha( RpMaterial *material, RwBool useFrameBufferAlpha )
{ using namespace rw;
	MatFX *mfx = MatFX::get(material);
	mfx->setEnvFBAlpha(useFrameBufferAlpha);
	return material;
}
inline RpMaterial *RpMatFXMaterialSetEnvMapCoefficient( RpMaterial *material, RwReal coef )
{ using namespace rw;
	MatFX *mfx = MatFX::get(material);
	mfx->setEnvCoefficient(coef);
	return material;
}
inline RwReal RpMatFXMaterialGetEnvMapCoefficient( const RpMaterial *material )
{ using namespace rw;
	MatFX *mfx = MatFX::get(material);
	return mfx->getEnvCoefficient();
}



inline RwBool RpHAnimPluginAttach(void) { using namespace rw;
	registerHAnimPlugin();
	return true;
}

inline RwInt32 RpHAnimFrameGetID(RwFrame *frame) { using namespace rw; return HAnimData::get(frame)->id; }

inline RwInt32 RpHAnimIDGetIndex(RpHAnimHierarchy *hierarchy, RwInt32 ID) { using namespace rw; return hierarchy->getIndex(ID); }

inline RwBool RpHAnimFrameSetHierarchy(RwFrame *frame, RpHAnimHierarchy *hierarchy) { using namespace rw; HAnimData::get(frame)->hierarchy = hierarchy; return true; }
inline RpHAnimHierarchy *RpHAnimFrameGetHierarchy(RwFrame *frame) { using namespace rw; return HAnimHierarchy::get(frame); }

inline RpHAnimHierarchy *RpHAnimHierarchySetFlags(RpHAnimHierarchy *hierarchy, RpHAnimHierarchyFlag flags) { using namespace rw; hierarchy->flags = flags; return hierarchy; }

inline RwBool RpHAnimHierarchySetCurrentAnim(RpHAnimHierarchy *hierarchy, RpHAnimAnimation *anim) { using namespace rw; hierarchy->interpolator->setCurrentAnim(anim); return true; }
inline RwBool RpHAnimHierarchyAddAnimTime(RpHAnimHierarchy *hierarchy, RwReal time) { using namespace rw; hierarchy->interpolator->addTime(time); return true; }

inline RwMatrix *RpHAnimHierarchyGetMatrixArray(RpHAnimHierarchy *hierarchy) { using namespace rw; return hierarchy->matrices; }
inline RwBool RpHAnimHierarchyUpdateMatrices(RpHAnimHierarchy *hierarchy) { using namespace rw; hierarchy->updateMatrices(); return true; }

inline RpHAnimAnimation *RpHAnimAnimationCreate(RwInt32 typeID, RwInt32 numFrames, RwInt32 flags, RwReal duration)
	{ using namespace rw; return Animation::create(AnimInterpolatorInfo::find(typeID), numFrames, flags, duration); }
inline RpHAnimAnimation  *RpHAnimAnimationDestroy(RpHAnimAnimation *animation) { using namespace rw; animation->destroy(); return animation; }






inline RwBool RpSkinPluginAttach(void) { using namespace rw;
	registerSkinPlugin();
	return true;
}

inline RwUInt32 RpSkinGetNumBones( RpSkin *skin ) { using namespace rw; return skin->numBones; }
inline const RwMatrixWeights *RpSkinGetVertexBoneWeights( RpSkin *skin ) { using namespace rw; return (RwMatrixWeights*)skin->weights; }
inline const RwUInt32 *RpSkinGetVertexBoneIndices( RpSkin *skin ) { using namespace rw; return (RwUInt32*)skin->indices; }
inline const RwMatrix *RpSkinGetSkinToBoneMatrices( RpSkin *skin ) { using namespace rw; return (const RwMatrix*)skin->inverseMatrices; }

inline RpSkin *RpSkinGeometryGetSkin( RpGeometry *geometry ) { using namespace rw; return Skin::get(geometry); }

inline RpAtomic *RpSkinAtomicSetHAnimHierarchy( RpAtomic *atomic, RpHAnimHierarchy *hierarchy ) { using namespace rw; Skin::setHierarchy(atomic, hierarchy); return atomic; }
inline RpHAnimHierarchy *RpSkinAtomicGetHAnimHierarchy( const RpAtomic *atomic ) { using namespace rw; return Skin::getHierarchy(atomic); }

inline RwImage * RtBMPImageWrite(RwImage *image, const RwChar *imageName)
{ using namespace rw;
#ifndef _WIN32
	char *r = casepath(imageName, true);
	if (r) {
		rw::writeBMP(image, r);
		free(r);
	} else {
		rw::writeBMP(image, imageName);
	}
	
#else
	rw::writeBMP(image, imageName);
#endif
	return image;
}
inline RwImage * RtBMPImageRead(const RwChar *imageName)
{ using namespace rw;
#ifndef _WIN32
	RwImage *image;
	char *r = casepath(imageName, true);
	if (r) {
		image = rw::readBMP(r);
		free(r);
	} else {
		image = rw::readBMP(imageName);
	}
	return image;

#else
	return rw::readBMP(imageName);
#endif
}


inline RwImage * RtPNGImageWrite(RwImage *image, const RwChar *imageName)
{ using namespace rw;
#ifndef _WIN32
	char *r = casepath(imageName, true);
	if (r) {
		rw::writePNG(image, r);
		free(r);
	} else {
		rw::writePNG(image, imageName);
	}
	
#else
	rw::writePNG(image, imageName);
#endif
	return image;
}
inline RwImage * RtPNGImageRead(const RwChar *imageName)
{ using namespace rw;
#ifndef _WIN32
	RwImage *image;
	char *r = casepath(imageName, true);
	if (r) {
		image = rw::readPNG(r);
		free(r);
	} else {
		image = rw::readPNG(imageName);
	}
	return image;

#else
	return rw::readPNG(imageName);
#endif
}


inline RtQuat *RtQuatRotate(RtQuat * quat, const RwV3d * axis, RwReal angle, RwOpCombineType combineOp) { using namespace rw; return (RtQuat*)((rw::Quat*)quat)->rotate(axis, angle/180.0f*3.14159f, (CombineOp)combineOp); }
inline void RtQuatConvertToMatrix(const RtQuat * const qpQuat, RwMatrix * const mpMatrix) { using namespace rw; mpMatrix->rotate(*(rw::Quat*)qpQuat, COMBINEREPLACE); }



inline RwBool       RtCharsetOpen(void) { using namespace rw; return Charset::open(); }
inline void         RtCharsetClose(void) { using namespace rw; return Charset::close(); }
inline RtCharset   *RtCharsetPrint(RtCharset * charSet, const RwChar * string, RwInt32 x, RwInt32 y) { using namespace rw; charSet->print(string, x, y, true); return charSet; }
inline RtCharset   *RtCharsetPrintBuffered(RtCharset * charSet, const RwChar * string, RwInt32 x, RwInt32 y, RwBool hideSpaces) { using namespace rw; charSet->printBuffered(string, x, y, hideSpaces); return charSet; }
inline RwBool       RtCharsetBufferFlush(void) { using namespace rw; Charset::flushBuffer(); return true; }
inline RtCharset   *RtCharsetSetColors(RtCharset * charSet, const RwRGBA * foreGround, const RwRGBA * backGround) { using namespace rw; return charSet->setColors(foreGround, backGround); }
inline RtCharset   *RtCharsetGetDesc(RtCharset * charset, RtCharsetDesc * desc) { using namespace rw; *desc = charset->desc; return charset; }
inline RtCharset   *RtCharsetCreate(const RwRGBA * foreGround, const RwRGBA * backGround) { using namespace rw; return Charset::create(foreGround, backGround); }
inline RwBool       RtCharsetDestroy(RtCharset * charSet) { using namespace rw; charSet->destroy(); return true; }




inline RwInt8      RpAnisotGetMaxSupportedMaxAnisotropy(void) { using namespace rw; return rw::getMaxSupportedMaxAnisotropy(); }
inline RwTexture    *RpAnisotTextureSetMaxAnisotropy(RwTexture *tex, RwInt8 val) { using namespace rw; tex->setMaxAnisotropy(val); return tex; }
inline RwInt8       RpAnisotTextureGetMaxAnisotropy(RwTexture *tex) { using namespace rw; return tex->getMaxAnisotropy(); }
inline RwBool       RpAnisotPluginAttach(void) { using namespace rw; rw::registerAnisotropyPlugin(); return true; }
