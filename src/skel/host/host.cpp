// Skeleton for the Rust host (rust/revc): SDL3 window, GL context, input and the process
// entry point live in Rust; this file keeps the game-side skeleton logic. Derived from
// skel/glfw/glfw.cpp with the Win32/Switch/Apple/X11 branches removed.
#if defined RW_GL3 && defined LIBRW_HOST


long _dwOperatingSystemVersion;
#include <sys/sysinfo.h>
#include <errno.h>
#include <locale.h>
#include <signal.h>
#include <stddef.h>

#include "common.h"
#include <stdio.h>
#include "rwcore.h"
#include "skeleton.h"
#include "platform.h"
#include "crossplatform.h"

#include "main.h"
#include "FileMgr.h"
#include "Text.h"
#include "Pad.h"
#include "Timer.h"
#include "DMAudio.h"
#include "ControllerConfig.h"
#include "Frontend.h"
#include "Game.h"
#include "PCSave.h"
#include "MemoryCard.h"
#include "Sprite2d.h"
#include "AnimViewer.h"
#include "Font.h"
#include "MemoryMgr.h"

// This is defined on project-level, via premake5 or cmake


#define MAX_SUBSYSTEMS		(16)

rw::EngineOpenParams openParams;

// Provided by the Rust host: window/GL services for librw's HOST backend.
extern "C" const rw::gl3::HostWindowApi *revc_host_window_api(void);

static RwBool		  ForegroundApp = TRUE;
static RwBool		  WindowIconified = FALSE;
static RwBool		  WindowFocused = TRUE;

static RwBool		  RwInitialised = FALSE;

static RwSubSystemInfo GsubSysInfo[MAX_SUBSYSTEMS];
static RwInt32		GnumSubSystems = 0;
static RwInt32		GcurSel = 0, GcurSelVM = 0;

static RwBool useDefault;

// What is that for anyway?
#ifndef IMPROVED_VIDEOMODE
static RwBool defaultFullscreenRes = TRUE;
#else
static RwBool defaultFullscreenRes = FALSE;
static RwInt32 bestWndMode = -1;
#endif

// Window/input events arrive from the Rust host as revc_on_* calls (hostapi.h) during
// hostPollEvents(). Like GLFW's callbacks, they are only acted on after psPostRWinit().
static bool HostCallbacksEnabled = false;

static psGlobalType PsGlobal;


#define PSGLOBAL(var) (((psGlobalType *)(RsGlobal.ps))->var)

size_t _dwMemAvailPhys;
RwUInt32 gGameState;

#ifdef DETECT_JOYSTICK_MENU
char gSelectedJoystickName[128] = "";
#endif

/*
 *****************************************************************************
 */
void _psCreateFolder(const char *path)
{
	struct stat info;
	char fullpath[PATH_MAX];
	realpath(path, fullpath);

	if (lstat(fullpath, &info) != 0) {
		if (errno == ENOENT || (errno != EACCES && !S_ISDIR(info.st_mode))) {
			mkdir(fullpath, 0755);
		}
	}
}

/*
 *****************************************************************************
 */
const char *_psGetUserFilesFolder()
{
	static char szUserFiles[256];
	strcpy(szUserFiles, "userfiles");
	_psCreateFolder(szUserFiles);
	return szUserFiles;
}

/*
 *****************************************************************************
 */
RwBool
psCameraBeginUpdate(RwCamera *camera)
{
	if ( !RwCameraBeginUpdate(Scene.camera) )
	{
		ForegroundApp = FALSE;
		RsEventHandler(rsACTIVATE, (void *)FALSE);
		return FALSE;
	}
	
	return TRUE;
}

/*
 *****************************************************************************
 */
void
psCameraShowRaster(RwCamera *camera)
{
#ifdef LEGACY_MENU_OPTIONS
	if (FrontEndMenuManager.m_PrefsVsync || FrontEndMenuManager.m_bMenuActive)
#else
	if (FrontEndMenuManager.m_PrefsFrameLimiter || FrontEndMenuManager.m_bMenuActive)
#endif
		RwCameraShowRaster(camera, PSGLOBAL(window), rwRASTERFLIPWAITVSYNC);
	else
		RwCameraShowRaster(camera, PSGLOBAL(window), rwRASTERFLIPDONTWAIT);

	return;
}

/*
 *****************************************************************************
 */
RwImage *
psGrabScreen(RwCamera *pCamera)
{
#ifndef LIBRW
	RwRaster *pRaster = RwCameraGetRaster(pCamera);
	if (RwImage *pImage = RwImageCreate(pRaster->width, pRaster->height, 32)) {
		RwImageAllocatePixels(pImage);
		RwImageSetFromRaster(pImage, pRaster);
		return pImage;
	}
#else
	rw::Image *image = RwCameraGetRaster(pCamera)->toImage();
	image->removeMask();
	if(image)
		return image;
#endif
	return nil;
}

/*
 *****************************************************************************
 */
double
psTimer(void)
{
	struct timespec start; 
#if defined(CLOCK_MONOTONIC_RAW)
	clock_gettime(CLOCK_MONOTONIC_RAW, &start);
#elif defined(CLOCK_MONOTONIC_FAST)
	clock_gettime(CLOCK_MONOTONIC_FAST, &start);
#else
	clock_gettime(CLOCK_MONOTONIC, &start);
#endif
	return start.tv_sec * 1000.0 + start.tv_nsec/1000000.0;
}


/*
 *****************************************************************************
 */
void
psMouseSetPos(RwV2d *pos)
{
	hostSetCursorPos(pos->x, pos->y);
	
	PSGLOBAL(lastMousePos.x) = (RwInt32)pos->x;

	PSGLOBAL(lastMousePos.y) = (RwInt32)pos->y;

	return;
}

/*
 *****************************************************************************
 */
RwMemoryFunctions*
psGetMemoryFunctions(void)
{
#ifdef USE_CUSTOM_ALLOCATOR
	return &memFuncs;
#else
	return nil;
#endif
}

/*
 *****************************************************************************
 */
RwBool
psInstallFileSystem(void)
{
	return (TRUE);
}


/*
 *****************************************************************************
 */
RwBool
psNativeTextureSupport(void)
{
	return true;
}

/*
 *****************************************************************************
 */
#ifdef UNDER_CE
#define CMDSTR	LPWSTR
#else
#define CMDSTR	LPSTR
#endif

/*
 *****************************************************************************
 */

static void _psInitializeVibration() {}
static void _psHandleVibration() {}

/*
 *****************************************************************************
 */
RwBool
psInitialize(void)
{
	PsGlobal.lastMousePos.x = PsGlobal.lastMousePos.y = 0.0f;

	RsGlobal.ps = &PsGlobal;
	
	PsGlobal.fullScreen = FALSE;
	PsGlobal.cursorIsInWindow = FALSE;
	WindowFocused = TRUE;
	WindowIconified = FALSE;
	
	PsGlobal.joy1id	= -1;
	PsGlobal.joy2id	= -1;

	CFileMgr::Initialise();
	
#ifdef PS2_MENU
	CPad::Initialise();
	CPad::GetPad(0)->Mode = 0;

	CGame::frenchGame = false;
	CGame::germanGame = false;
	CGame::nastyGame = true;
	CMenuManager::m_PrefsAllowNastyGame = true;

	// Mandatory for Linux(Unix? Posix?) to set lang. to environment lang.
	setlocale(LC_ALL, "");	

	char *systemLang, *keyboardLang;

	systemLang = setlocale (LC_ALL, NULL);
	keyboardLang = setlocale (LC_CTYPE, NULL);
	
	short lang;
	lang = !strncmp(systemLang, "fr_",3) ? LANG_FRENCH :
					!strncmp(systemLang, "de_",3) ? LANG_GERMAN :
					!strncmp(systemLang, "en_",3) ? LANG_ENGLISH :
					!strncmp(systemLang, "it_",3) ? LANG_ITALIAN :
					!strncmp(systemLang, "es_",3) ? LANG_SPANISH :
					LANG_OTHER;

	if ( lang  == LANG_ITALIAN )
		CMenuManager::m_PrefsLanguage = CMenuManager::LANGUAGE_ITALIAN;
	else if ( lang  == LANG_SPANISH )
		CMenuManager::m_PrefsLanguage = CMenuManager::LANGUAGE_SPANISH;
	else if ( lang  == LANG_GERMAN )
	{
		CGame::germanGame = true;
		CGame::nastyGame = false;
		CMenuManager::m_PrefsAllowNastyGame = false;
		CMenuManager::m_PrefsLanguage = CMenuManager::LANGUAGE_GERMAN;
	}
	else if ( lang  == LANG_FRENCH )
	{
		CGame::frenchGame = true;
		CGame::nastyGame = false;
		CMenuManager::m_PrefsAllowNastyGame = false;
		CMenuManager::m_PrefsLanguage = CMenuManager::LANGUAGE_FRENCH;
	}
	else
		CMenuManager::m_PrefsLanguage = CMenuManager::LANGUAGE_AMERICAN;

	FrontEndMenuManager.InitialiseMenuContentsAfterLoadingGame();

	TheMemoryCard.Init();
#else
	C_PcSave::SetSaveDirectory(_psGetUserFilesFolder());
	
	InitialiseLanguage();

#endif

	_psInitializeVibration();
	
	gGameState = GS_START_UP;
	TRACE("gGameState = GS_START_UP");
	_dwOperatingSystemVersion = OS_WINXP; // To fool other classes

	
#ifndef PS2_MENU
	FrontEndMenuManager.LoadSettings();
#endif


 	struct sysinfo systemInfo;
	sysinfo(&systemInfo);
	_dwMemAvailPhys = systemInfo.freeram;
	debug("Physical memory size %u\n", systemInfo.totalram);
	debug("Available physical memory %u\n", systemInfo.freeram);
	_dwOperatingSystemVersion = OS_WINXP; // To fool other classes
  
  TheText.Unload();

	return TRUE;
}


/*
 *****************************************************************************
 */
void
psTerminate(void)
{
	return;
}

/*
 *****************************************************************************
 */
static RwChar **_VMList;

RwInt32 _psGetNumVideModes()
{
	return RwEngineGetNumVideoModes();
}

/*
 *****************************************************************************
 */
RwBool _psFreeVideoModeList()
{
	RwInt32 numModes;
	RwInt32 i;
	
	numModes = _psGetNumVideModes();
	
	if ( _VMList == nil )
		return TRUE;
	
	for ( i = 0; i < numModes; i++ )
	{
		RwFree(_VMList[i]);
	}
	
	RwFree(_VMList);
	
	_VMList = nil;
	
	return TRUE;
}
							
/*
 *****************************************************************************
 */							
RwChar **_psGetVideoModeList()
{
	RwInt32 numModes;
	RwInt32 i;
	
	if ( _VMList != nil )
	{
		return _VMList;
	}
	
	numModes = RwEngineGetNumVideoModes();
	
	_VMList = (RwChar **)RwCalloc(numModes, sizeof(RwChar*));
	
	for ( i = 0; i < numModes; i++	)
	{
		RwVideoMode			vm;
		
		RwEngineGetVideoModeInfo(&vm, i);
		
		if ( vm.flags & rwVIDEOMODEEXCLUSIVE )
		{
			_VMList[i] = (RwChar*)RwCalloc(100, sizeof(RwChar));
			rwsprintf(_VMList[i],"%d X %d X %d", vm.width, vm.height, vm.depth);
		}
		else
			_VMList[i] = nil;
	}
	
	return _VMList;
}

/*
 *****************************************************************************
 */
void _psSelectScreenVM(RwInt32 videoMode)
{
	RwTexDictionarySetCurrent( nil );
	
	FrontEndMenuManager.UnloadTextures();
	
	if (!_psSetVideoMode(RwEngineGetCurrentSubSystem(), videoMode))
	{
		RsGlobal.quit = TRUE;

		printf("ERROR: Failed to select new screen resolution\n");
	}
	else
		FrontEndMenuManager.LoadAllTextures();
}

/*
 *****************************************************************************
 */
void _psSwitchScreenModeLive(void)
{
	bool windowed = !!FrontEndMenuManager.m_nPrefsWindowed;
	RwVideoMode vm;
	RwEngineGetVideoModeInfo(&vm, FrontEndMenuManager.m_nPrefsVideoMode);

	if (!hostSetFullscreen(!windowed, vm.width, vm.height)) {
		printf("ERROR: Failed to switch to %s mode\n", windowed ? "windowed" : "fullscreen");
		return;
	}

	// Keep librw's mode index in step; _InputMouseNeedsExclusive() reads it.
	GcurSelVM = windowed ? (bestWndMode >= 0 ? bestWndMode : 0) : FrontEndMenuManager.m_nPrefsVideoMode;
	RwEngineSetVideoMode(GcurSelVM);
	PSGLOBAL(fullScreen) = !windowed;
	// The new size arrives as a revc_on_resize() event.

	_InputInitialiseMouse(!FrontEndMenuManager.m_bMenuActive && _InputMouseNeedsExclusive());
}

void _psApplyResolutionLive(void)
{
	_psSwitchScreenModeLive();
	if (FrontEndMenuManager.m_nPrefsWindowed) {
		RwVideoMode vm;
		RwEngineGetVideoModeInfo(&vm, FrontEndMenuManager.m_nPrefsVideoMode);
		hostSetWindowSize(vm.width, vm.height);
	}
}

extern "C" void revc_on_toggle_fullscreen(void)
{
	if (!HostCallbacksEnabled)
		return;
	FrontEndMenuManager.m_nPrefsWindowed = !FrontEndMenuManager.m_nPrefsWindowed;
	FrontEndMenuManager.SaveSettings();
	_psSwitchScreenModeLive();
}

/*
 *****************************************************************************
 */

RwBool IsForegroundApp()
{
	return !!ForegroundApp;
}
/*
UINT GetBestRefreshRate(UINT width, UINT height, UINT depth)
{
	LPDIRECT3D8 d3d = Direct3DCreate8(D3D_SDK_VERSION);
	
	ASSERT(d3d != nil);
	
	UINT refreshRate = INT_MAX;
	D3DFORMAT format;

	if ( depth == 32 )
		format = D3DFMT_X8R8G8B8;
	else if ( depth == 24 )
		format = D3DFMT_R8G8B8;
	else
		format = D3DFMT_R5G6B5;
	
	UINT modeCount = d3d->GetAdapterModeCount(GcurSel);
	
	for ( UINT i = 0; i < modeCount; i++ )
	{
		D3DDISPLAYMODE mode;
		
		d3d->EnumAdapterModes(GcurSel, i, &mode);
		
		if ( mode.Width == width && mode.Height == height && mode.Format == format )
		{
			if ( mode.RefreshRate == 0 )
				return 0;

			if ( mode.RefreshRate < refreshRate && mode.RefreshRate >= 60 )
				refreshRate = mode.RefreshRate;
		}
	}
	
#ifdef FIX_BUGS
	d3d->Release();
#endif
	
	if ( refreshRate == -1 )
		return -1;

	return refreshRate;
}
*/
/*
 *****************************************************************************
 */
RwBool
psSelectDevice()
{
	RwVideoMode			vm;
	RwInt32				subSysNum;
	RwInt32				AutoRenderer = 0;
	

	RwBool modeFound = FALSE;
	
	if ( !useDefault )
	{
		GnumSubSystems = RwEngineGetNumSubSystems();
		if ( !GnumSubSystems )
		{
			 return FALSE;
		}
		
		/* Just to be sure ... */
		GnumSubSystems = (GnumSubSystems > MAX_SUBSYSTEMS) ? MAX_SUBSYSTEMS : GnumSubSystems;
		
		/* Get the names of all the sub systems */
		for (subSysNum = 0; subSysNum < GnumSubSystems; subSysNum++)
		{
			RwEngineGetSubSystemInfo(&GsubSysInfo[subSysNum], subSysNum);
		}
		
		/* Get the default selection */
		GcurSel = RwEngineGetCurrentSubSystem();
#ifdef IMPROVED_VIDEOMODE
		if(FrontEndMenuManager.m_nPrefsSubsystem < GnumSubSystems)
			GcurSel = FrontEndMenuManager.m_nPrefsSubsystem;
#endif
	}
	
	/* Set the driver to use the correct sub system */
	if (!RwEngineSetSubSystem(GcurSel))
	{
		return FALSE;
	}

#ifdef IMPROVED_VIDEOMODE
	FrontEndMenuManager.m_nPrefsSubsystem = GcurSel;
#endif

#ifndef IMPROVED_VIDEOMODE
	if ( !useDefault )
	{
		if ( _psGetVideoModeList()[FrontEndMenuManager.m_nDisplayVideoMode] && FrontEndMenuManager.m_nDisplayVideoMode )
		{
			FrontEndMenuManager.m_nPrefsVideoMode = FrontEndMenuManager.m_nDisplayVideoMode;
			GcurSelVM = FrontEndMenuManager.m_nDisplayVideoMode;
		}
		else
		{
#ifdef DEFAULT_NATIVE_RESOLUTION
			// get the native video mode
			HDC hDevice = GetDC(NULL);
			int w = GetDeviceCaps(hDevice, HORZRES);
			int h = GetDeviceCaps(hDevice, VERTRES);
			int d = GetDeviceCaps(hDevice, BITSPIXEL);
#else
			const int w = 640;
			const int h = 480;
			const int d = 16;
#endif
			while ( !modeFound && GcurSelVM < RwEngineGetNumVideoModes() )
			{
				RwEngineGetVideoModeInfo(&vm, GcurSelVM);
				if ( defaultFullscreenRes	&& vm.width	 != w 
											|| vm.height != h
											|| vm.depth	 != d
											|| !(vm.flags & rwVIDEOMODEEXCLUSIVE) )
					++GcurSelVM;
				else
					modeFound = TRUE;
			}
			
			if ( !modeFound )
			{
#ifdef DEFAULT_NATIVE_RESOLUTION
				GcurSelVM = 1;
#else
				printf("WARNING: Cannot find 640x480 video mode, selecting device cancelled\n");
				return FALSE;
#endif
			}
		}
	}
#else
	if ( !useDefault )
	{
		if(FrontEndMenuManager.m_nPrefsWidth == 0 ||
		   FrontEndMenuManager.m_nPrefsHeight == 0 ||
		   FrontEndMenuManager.m_nPrefsDepth == 0){
			// Defaults if nothing specified
			int deskW, deskH, deskHz;
			hostGetDesktopMode(&deskW, &deskH, &deskHz);
			FrontEndMenuManager.m_nPrefsWidth = deskW;
			FrontEndMenuManager.m_nPrefsHeight = deskH;
			FrontEndMenuManager.m_nPrefsDepth = 32;
			FrontEndMenuManager.m_nPrefsWindowed = 0;
		}

		// Find the videomode that best fits what we got from the settings file
		RwInt32 bestFsMode = -1;
		RwInt32 bestWidth = -1;
		RwInt32 bestHeight = -1;
		RwInt32 bestDepth = -1;
		for(GcurSelVM = 0; GcurSelVM < RwEngineGetNumVideoModes(); GcurSelVM++){
			RwEngineGetVideoModeInfo(&vm, GcurSelVM);

			if (!(vm.flags & rwVIDEOMODEEXCLUSIVE)){
				bestWndMode = GcurSelVM;
			} else {
				// try the largest one that isn't larger than what we wanted
				if(vm.width >= bestWidth && vm.width <= FrontEndMenuManager.m_nPrefsWidth &&
				   vm.height >= bestHeight && vm.height <= FrontEndMenuManager.m_nPrefsHeight &&
				   vm.depth >= bestDepth && vm.depth <= FrontEndMenuManager.m_nPrefsDepth){
					bestWidth = vm.width;
					bestHeight = vm.height;
					bestDepth = vm.depth;
					bestFsMode = GcurSelVM;
				}
			}
		}

		if(bestFsMode < 0){
			printf("WARNING: Cannot find desired video mode, selecting device cancelled\n");
			return FALSE;
		}
		GcurSelVM = bestFsMode;

		FrontEndMenuManager.m_nDisplayVideoMode = GcurSelVM;
		FrontEndMenuManager.m_nPrefsVideoMode = FrontEndMenuManager.m_nDisplayVideoMode;

		FrontEndMenuManager.m_nSelectedScreenMode = FrontEndMenuManager.m_nPrefsWindowed;
	}
#endif

	RwEngineGetVideoModeInfo(&vm, GcurSelVM);

#ifdef IMPROVED_VIDEOMODE
	if (FrontEndMenuManager.m_nPrefsWindowed)
		GcurSelVM = bestWndMode;

	// Now GcurSelVM is 0 but vm has sizes(and fullscreen flag) of the video mode we want, that's why we changed the rwVIDEOMODEEXCLUSIVE conditions below
	FrontEndMenuManager.m_nPrefsWidth = vm.width;
	FrontEndMenuManager.m_nPrefsHeight = vm.height;
	FrontEndMenuManager.m_nPrefsDepth = vm.depth;
#endif

#ifndef PS2_MENU
	FrontEndMenuManager.m_nCurrOption = 0;
#endif
	
	/* Set up the video mode and set the apps window
	* dimensions to match */
	if (!RwEngineSetVideoMode(GcurSelVM))
	{
		return FALSE;
	}
	/*
	TODO
	if (vm.flags & rwVIDEOMODEEXCLUSIVE)
	{
		debug("%dx%dx%d", vm.width, vm.height, vm.depth);
		
		UINT refresh = GetBestRefreshRate(vm.width, vm.height, vm.depth);
		
		if ( refresh != (UINT)-1 )
		{
			debug("refresh %d", refresh);
			RwD3D8EngineSetRefreshRate((RwUInt32)refresh);
		}
	}
	*/
#ifndef IMPROVED_VIDEOMODE
	if (vm.flags & rwVIDEOMODEEXCLUSIVE)
	{
		RsGlobal.maximumWidth = vm.width;
		RsGlobal.maximumHeight = vm.height;
		RsGlobal.width = vm.width;
		RsGlobal.height = vm.height;
		
		PSGLOBAL(fullScreen) = TRUE;
	}
#else
		RsGlobal.maximumWidth = FrontEndMenuManager.m_nPrefsWidth;
		RsGlobal.maximumHeight = FrontEndMenuManager.m_nPrefsHeight;
		RsGlobal.width = FrontEndMenuManager.m_nPrefsWidth;
		RsGlobal.height = FrontEndMenuManager.m_nPrefsHeight;
		
		PSGLOBAL(fullScreen) = !FrontEndMenuManager.m_nPrefsWindowed;
#endif

#ifdef MULTISAMPLING
	RwD3D8EngineSetMultiSamplingLevels(1 << FrontEndMenuManager.m_nPrefsMSAALevel);
#endif
	return TRUE;
}


bool IsThisJoystickBlacklisted(int i)
{
#ifndef DETECT_JOYSTICK_MENU
	return false;
#else
	if (hostJoystickIsGamepad(i))
		return false;

	const char* joyname = hostGetJoystickName(i);
	if (joyname == nil)
		return true;

	if (gSelectedJoystickName[0] != '\0' &&
		strncmp(joyname, gSelectedJoystickName, strlen(gSelectedJoystickName)) == 0)
		return false;

	return true;
#endif
}

void _InputInitialiseJoys()
{
	PSGLOBAL(joy1id) = -1;
	PSGLOBAL(joy2id) = -1;

	// Load our gamepad mappings.
#define SDL_GAMEPAD_DB_PATH "gamecontrollerdb.txt"
	FILE *f = fopen(SDL_GAMEPAD_DB_PATH, "rb");
	if (f) {
		fseek(f, 0, SEEK_END);
		size_t fsize = ftell(f);
		fseek(f, 0, SEEK_SET);

		char *db = (char*)malloc(fsize + 1);
		if (fread(db, 1, fsize, f) == fsize) {
			db[fsize] = '\0';

			if (!hostUpdateGamepadMappings(db))
				Error("hostUpdateGamepadMappings didn't succeed, check " SDL_GAMEPAD_DB_PATH ".\n");
		} else
			Error("fread on " SDL_GAMEPAD_DB_PATH " wasn't successful.\n");

		free(db);
		fclose(f);
	} else
		printf("You don't seem to have copied " SDL_GAMEPAD_DB_PATH " file from reVC/gamefiles to GTA: Vice City directory. Some gamepads may not be recognized.\n");

#undef SDL_GAMEPAD_DB_PATH

	// But always overwrite it with the one in SDL_GAMECONTROLLERCONFIG.
	char const* EnvControlConfig = getenv("SDL_GAMECONTROLLERCONFIG");
	if (EnvControlConfig != nil) {
		hostUpdateGamepadMappings(EnvControlConfig);
	}

	for (int i = 0; i <= HOST_JOYSTICK_LAST; i++) {
		if (hostJoystickPresent(i) && !IsThisJoystickBlacklisted(i)) {
			if (PSGLOBAL(joy1id) == -1)
				PSGLOBAL(joy1id) = i;
			else if (PSGLOBAL(joy2id) == -1)
				PSGLOBAL(joy2id) = i;
			else
				break;
		}
	}

	if (PSGLOBAL(joy1id) != -1) {
		int count;
		hostGetJoystickButtons(PSGLOBAL(joy1id), &count);
#ifdef DETECT_JOYSTICK_MENU
		strcpy(gSelectedJoystickName, hostGetJoystickName(PSGLOBAL(joy1id)));
#endif
		ControlsManager.InitDefaultControlConfigJoyPad(count);
	}
}

int lastCursorMode = HOST_CURSOR_HIDDEN;
long _InputInitialiseMouse(bool exclusive)
{
	// Disabled = keep cursor centered and hide
	lastCursorMode = exclusive ? HOST_CURSOR_DISABLED : HOST_CURSOR_HIDDEN;
	hostSetCursorMode(lastCursorMode);
	return 0;
}

void _InputShutdownMouse()
{
	// Not needed
}

// Not really "needs exclusive", but more like "needs to change mode"
bool _InputMouseNeedsExclusive()
{
	// That was the cause of infamous mouse bug on Win.
	
	RwVideoMode vm;
	RwEngineGetVideoModeInfo(&vm, GcurSelVM);

	// If windowed, free the cursor on menu(where this func. is called and DISABLED-HIDDEN transition is done accordingly)
	// If it's fullscreen, be sure that it didn't stuck on HIDDEN.
	return !(vm.flags & rwVIDEOMODEEXCLUSIVE) || lastCursorMode == HOST_CURSOR_HIDDEN;
}

void psPostRWinit(void)
{
	RwVideoMode vm;
	RwEngineGetVideoModeInfo(&vm, GcurSelVM);

	HostCallbacksEnabled = true;

	_InputInitialiseJoys();
	_InputInitialiseMouse(false);

	if(!(vm.flags & rwVIDEOMODEEXCLUSIVE))
		hostSetWindowSize(RsGlobal.maximumWidth, RsGlobal.maximumHeight);

	// Make sure all keys are released
	CPad::GetPad(0)->Clear(true);
	CPad::GetPad(1)->Clear(true);
}

/*
 *****************************************************************************
 */
RwBool _psSetVideoMode(RwInt32 subSystem, RwInt32 videoMode)
{
	RwInitialised = FALSE;
	
	RsEventHandler(rsRWTERMINATE, nil);
	
	GcurSel = subSystem;
	GcurSelVM = videoMode;
	
	useDefault = TRUE;
	
	if ( RsEventHandler(rsRWINITIALIZE, &openParams) == rsEVENTERROR )
		return FALSE;

	RwInitialised = TRUE;
	useDefault = FALSE;
	
	RwRect r;
	
	r.x = 0;
	r.y = 0;
	r.w = RsGlobal.maximumWidth;
	r.h = RsGlobal.maximumHeight;

	RsEventHandler(rsCAMERASIZE, &r);
	
	psPostRWinit();
	
	return TRUE;
}
 
 
/*
 *****************************************************************************
 */
static RwChar **
CommandLineToArgv(RwChar *cmdLine, RwInt32 *argCount)
{
	RwInt32 numArgs = 0;
	RwBool inArg, inString;
	RwInt32 i, len;
	RwChar *res, *str, **aptr;

	len = strlen(cmdLine);

	/* 
	 * Count the number of arguments...
	 */
	inString = FALSE;
	inArg = FALSE;

	for(i=0; i<=len; i++)
	{
		if( cmdLine[i] == '"' )
		{
			inString = !inString;
		}

		if( (cmdLine[i] <= ' ' && !inString) || i == len )
		{
			if( inArg ) 
			{
				inArg = FALSE;
				
				numArgs++;
			}
		} 
		else if( !inArg )
		{
			inArg = TRUE;
		}
	}

	/* 
	 * Allocate memory for result...
	 */
	res = (RwChar *)malloc(sizeof(RwChar *) * numArgs + len + 1);
	str = res + sizeof(RwChar *) * numArgs;
	aptr = (RwChar **)res;

	strcpy(str, cmdLine);

	/*
	 * Walk through cmdLine again this time setting pointer to each arg...
	 */
	inArg = FALSE;
	inString = FALSE;

	for(i=0; i<=len; i++)
	{
		if( cmdLine[i] == '"' )
		{
			inString = !inString;
		}

		if( (cmdLine[i] <= ' ' && !inString) || i == len )
		{
			if( inArg ) 
			{
				if( str[i-1] == '"' )
				{
					str[i-1] = '\0';
				}
				else
				{
					str[i] = '\0';
				}
				
				inArg = FALSE;
			}
		} 
		else if( !inArg && cmdLine[i] != '"' )
		{
			inArg = TRUE; 
			
			*aptr++ = &str[i];
		}
	}

	*argCount = numArgs;

	return (RwChar **)res;
}

/*
 *****************************************************************************
 */
void InitialiseLanguage()
{
	// Mandatory for Linux(Unix? Posix?) to set lang. to environment lang.
	setlocale(LC_ALL, "");	

	char *systemLang, *keyboardLang;

	systemLang = setlocale (LC_ALL, NULL);
	keyboardLang = setlocale (LC_CTYPE, NULL);
	
	short primUserLCID, primSystemLCID;
	primUserLCID = primSystemLCID = !strncmp(systemLang, "fr_",3) ? LANG_FRENCH :
					!strncmp(systemLang, "de_",3) ? LANG_GERMAN :
					!strncmp(systemLang, "en_",3) ? LANG_ENGLISH :
					!strncmp(systemLang, "it_",3) ? LANG_ITALIAN :
					!strncmp(systemLang, "es_",3) ? LANG_SPANISH :
					LANG_OTHER;

	short primLayout = !strncmp(keyboardLang, "fr_",3) ? LANG_FRENCH : (!strncmp(keyboardLang, "de_",3) ? LANG_GERMAN : LANG_ENGLISH);

	short subUserLCID, subSystemLCID;
	subUserLCID = subSystemLCID = !strncmp(systemLang, "en_AU",5) ? SUBLANG_ENGLISH_AUS : SUBLANG_OTHER;
	short subLayout = !strncmp(keyboardLang, "en_AU",5) ? SUBLANG_ENGLISH_AUS : SUBLANG_OTHER;

	if (   primUserLCID	  == LANG_GERMAN
		|| primSystemLCID == LANG_GERMAN
		|| primLayout	  == LANG_GERMAN )
	{
		CGame::nastyGame = false;
		FrontEndMenuManager.m_PrefsAllowNastyGame = false;
		CGame::germanGame = true;
	}
	
	if (   primUserLCID	  == LANG_FRENCH
		|| primSystemLCID == LANG_FRENCH
		|| primLayout	  == LANG_FRENCH )
	{
		CGame::nastyGame = false;
		FrontEndMenuManager.m_PrefsAllowNastyGame = false;
		CGame::frenchGame = true;
	}
	
	if (   subUserLCID	 == SUBLANG_ENGLISH_AUS
		|| subSystemLCID == SUBLANG_ENGLISH_AUS
		|| subLayout	 == SUBLANG_ENGLISH_AUS )
		CGame::noProstitutes = true;

#ifdef NASTY_GAME
	CGame::nastyGame = true;
	FrontEndMenuManager.m_PrefsAllowNastyGame = true;
	CGame::noProstitutes = false;
#endif
	
	int32 lang;
	
	switch ( primSystemLCID )
	{
		case LANG_GERMAN:
		{
			lang = LANG_GERMAN;
			break;
		}
		case LANG_FRENCH:
		{
			lang = LANG_FRENCH;
			break;
		}
		case LANG_SPANISH:
		{
			lang = LANG_SPANISH;
			break;
		}
		case LANG_ITALIAN:
		{
			lang = LANG_ITALIAN;
			break;
		}
		default:
		{
			lang = ( subSystemLCID == SUBLANG_ENGLISH_AUS ) ? -99 : LANG_ENGLISH;
			break;
		}
	}
	
	FrontEndMenuManager.OS_Language = primUserLCID;

	switch ( lang )
	{
		case LANG_GERMAN:
		{
			FrontEndMenuManager.m_PrefsLanguage = CMenuManager::LANGUAGE_GERMAN;
			break;
		}
		case LANG_SPANISH:
		{
			FrontEndMenuManager.m_PrefsLanguage = CMenuManager::LANGUAGE_SPANISH;
			break;
		}
		case LANG_FRENCH:
		{
			FrontEndMenuManager.m_PrefsLanguage = CMenuManager::LANGUAGE_FRENCH;
			break;
		}
		case LANG_ITALIAN:
		{
			FrontEndMenuManager.m_PrefsLanguage = CMenuManager::LANGUAGE_ITALIAN;
			break;
		}
		default:
		{
			FrontEndMenuManager.m_PrefsLanguage = CMenuManager::LANGUAGE_AMERICAN;
			break;
		}
	}

	// TODO this is needed for strcasecmp to work correctly across all languages, but can these cause other problems??
	setlocale(LC_CTYPE, "C");
	setlocale(LC_COLLATE, "C");
	setlocale(LC_NUMERIC, "C");

	TheText.Unload();
	TheText.Load();
}

/*
 *****************************************************************************
 */

void HandleExit()
{
	// We now handle terminate message always, why handle on some cases?
	return;
}

void terminateHandler(int sig, siginfo_t *info, void *ucontext) {
	RsGlobal.quit = TRUE;
}

#ifdef FLUSHABLE_STREAMING
void dummyHandler(int sig){
	// Don't kill the app pls
}
#endif

extern "C" void revc_on_resize(int width, int height) {
	if (!HostCallbacksEnabled)
		return;
	/*
	* Handle event to ensure window contents are displayed during re-size
	* as this can be disabled by the user, then if there is not enough
	* memory things don't work.
	*/
	/* redraw window */

	if (RwInitialised && gGameState == GS_PLAYING_GAME)
	{
		RsEventHandler(rsIDLE, (void *)TRUE);
	}

	if (RwInitialised && height > 0 && width > 0) {
		RwRect r;

		// TODO fix artifacts of resizing with mouse
		RsGlobal.maximumHeight = height;
		RsGlobal.maximumWidth = width;

		r.x = 0;
		r.y = 0;
		r.w = width;
		r.h = height;

		RsEventHandler(rsCAMERASIZE, &r);
	}
}

extern "C" void revc_on_scroll(double xoffset, double yoffset) {
#ifndef IGNORE_MOUSE_KEYBOARD
	if (HostCallbacksEnabled)
		PSGLOBAL(mouseWheel) = yoffset;
#endif
}

bool lshiftStatus = false;
bool rshiftStatus = false;

// The key table lives in the Rust host (rust/revc/src/host/keymap.rs).

// key is already translated to RsKeyCodes by the host; key repeats are not delivered.
extern "C" void
revc_on_key(int key, int down)
{
#ifndef IGNORE_MOUSE_KEYBOARD
	if (!HostCallbacksEnabled)
		return;
	RsKeyCodes ks = (RsKeyCodes)key;

	if (key == rsLSHIFT)
		lshiftStatus = !!down;

	if (key == rsRSHIFT)
		rshiftStatus = !!down;

	RsKeyboardEventHandler(down ? rsKEYDOWN : rsKEYUP, &ks);
#endif
}


// R* calls that in ControllerConfig, idk why
void
_InputTranslateShiftKeyUpDown(RsKeyCodes *rs) {
	RsKeyboardEventHandler(lshiftStatus ? rsKEYDOWN : rsKEYUP, &(*rs = rsLSHIFT));
	RsKeyboardEventHandler(rshiftStatus ? rsKEYDOWN : rsKEYUP, &(*rs = rsRSHIFT));
}

// TODO this only works in frontend(and luckily only frontend use this).
extern "C" void
revc_on_cursor(double xpos, double ypos) {
#ifdef IGNORE_MOUSE_KEYBOARD
	return;
#endif
	if (!HostCallbacksEnabled || !FrontEndMenuManager.m_bMenuActive)
		return;
	
	int winw, winh;
	hostGetWindowSize(&winw, &winh);
	if (winw <= 0 || winh <= 0)
		return;
	FrontEndMenuManager.m_nMouseTempPosX = xpos * (RsGlobal.maximumWidth / winw);
	FrontEndMenuManager.m_nMouseTempPosY = ypos * (RsGlobal.maximumHeight / winh);
}

extern "C" void
revc_on_cursor_enter(int entered) {
#ifndef IGNORE_MOUSE_KEYBOARD
	if (HostCallbacksEnabled)
		PSGLOBAL(cursorIsInWindow) = !!entered;
#endif
}

extern "C" void
revc_on_focus(int focused) {
	if (HostCallbacksEnabled)
		WindowFocused = !!focused;
}

extern "C" void
revc_on_iconify(int iconified) {
	if (HostCallbacksEnabled)
		WindowIconified = !!iconified;
}

/*
 *****************************************************************************
 */
#ifdef _WIN32
int PASCAL
WinMain(HINSTANCE instance,
	HINSTANCE prevInstance	__RWUNUSED__,
	CMDSTR cmdLine,
	int cmdShow)
{

	RwInt32 argc;
	RwChar** argv;
	SystemParametersInfo(SPI_SETFOREGROUNDLOCKTIMEOUT, 0, nil, SPIF_SENDCHANGE);

#ifndef MASTER
	if (strstr(cmdLine, "-console"))
	{
		AllocConsole();
		freopen("CONIN$", "r", stdin);
		freopen("CONOUT$", "w", stdout);
		freopen("CONOUT$", "w", stderr);
	}
#endif

#elif defined(REVC_RUST_MAIN)
// Rust owns main() (rust/revc); it calls this with the process argv.
extern "C" int
revc_cpp_main(int argc, char *argv[])
{
#else
// Called from Rust main() with the process argv.
extern "C" int
revc_cpp_main(int argc, char *argv[])
{
#endif
	RwV2d pos;
	RwInt32 i;

#ifdef USE_CUSTOM_ALLOCATOR
	InitMemoryMgr();
#endif

	struct sigaction act;
	act.sa_sigaction = terminateHandler;
	act.sa_flags = SA_SIGINFO;
	sigaction(SIGTERM, &act, NULL);
#ifdef FLUSHABLE_STREAMING
	struct sigaction sa;
	sigemptyset(&sa.sa_mask);
	sa.sa_handler = dummyHandler;
	sa.sa_flags = 0;
	sigaction(SIGUSR1, &sa, NULL);
#endif

	/* 
	 * Initialize the platform independent data.
	 * This will in turn initialize the platform specific data...
	 */
	if( RsEventHandler(rsINITIALIZE, nil) == rsEVENTERROR )
	{
		return FALSE;
	}

	for(i=1; i<argc; i++)
	{
		RsEventHandler(rsPREINITCOMMANDLINE, argv[i]);
	}

	/*
	 * Parameters to be used in RwEngineOpen / rsRWINITIALISE event
	 */

	openParams.width = RsGlobal.maximumWidth;
	openParams.height = RsGlobal.maximumHeight;
	openParams.windowtitle = RsGlobal.appName;
	openParams.window = &PSGLOBAL(window);
	openParams.host = revc_host_window_api();
	
	ControlsManager.MakeControllerActionsBlank();
	ControlsManager.InitDefaultControlConfiguration();

	/* 
	 * Initialize the 3D (RenderWare) components of the app...
	 */
	if( rsEVENTERROR == RsEventHandler(rsRWINITIALIZE, &openParams) )
	{
		RsEventHandler(rsTERMINATE, nil);

		return 0;
	}


	psPostRWinit();

	ControlsManager.InitDefaultControlConfigMouse(MousePointerStateHelper.GetMouseSetUp());


	/* 
	 * Parse command line parameters (except program name) one at 
	 * a time AFTER RenderWare initialization...
	 */
	for(i=1; i<argc; i++)
	{
		RsEventHandler(rsCOMMANDLINE, argv[i]);
	}

	/* 
	 * Force a camera resize event...
	 */
	{
		RwRect r;

		r.x = 0;
		r.y = 0;
		r.w = RsGlobal.maximumWidth;
		r.h = RsGlobal.maximumHeight;

		RsEventHandler(rsCAMERASIZE, &r);
	}

	{
		CFileMgr::SetDirMyDocuments();
		
#ifdef LOAD_INI_SETTINGS
		// At this point InitDefaultControlConfigJoyPad must have set all bindings to default and ms_padButtonsInited to number of detected buttons.
		// We will load stored bindings below, but let's cache ms_padButtonsInited before LoadINIControllerSettings and LoadSettings clears it,
		// so we can add new joy bindings **on top of** stored bindings.
		int connectedPadButtons = ControlsManager.ms_padButtonsInited;
#endif

		int32 gta3set = CFileMgr::OpenFile("gta_vc.set", "r");
		
		if ( gta3set )
		{
			ControlsManager.LoadSettings(gta3set);
			CFileMgr::CloseFile(gta3set);
		}
		
		CFileMgr::SetDir("");

#ifdef LOAD_INI_SETTINGS
		LoadINIControllerSettings();
		if (connectedPadButtons != 0)
			ControlsManager.InitDefaultControlConfigJoyPad(connectedPadButtons); // add (connected-saved) amount of new button assignments on top of ours

		// these have 2 purposes: creating .ini at the start, and adding newly introduced settings to old .ini at the start
		SaveINISettings();
		SaveINIControllerSettings();
#endif
	}
	

#ifdef PS2_MENU
	int32 r = TheMemoryCard.CheckCardStateAtGameStartUp(CARD_ONE);
	if (   r == CMemoryCard::ERR_DIRNOENTRY  || r == CMemoryCard::ERR_NOFORMAT
		&& r != CMemoryCard::ERR_OPENNOENTRY && r != CMemoryCard::ERR_NONE )
	{
		LoadingScreen(nil, nil, "loadsc0");
		
		TheText.Unload();
		TheText.Load();
		
		CFont::Initialise();
		
		FrontEndMenuManager.DrawMemoryCardStartUpMenus();
	}
#endif
	

	while ( TRUE )
	{
		RwInitialised = TRUE;
		
		/* 
		* Set the initial mouse position...
		*/
		pos.x = RsGlobal.maximumWidth * 0.5f;
		pos.y = RsGlobal.maximumHeight * 0.5f;

		RsMouseSetPos(&pos);
		
		/*
		* Enter the message processing loop...
		*/

#ifndef MASTER
		if (gbModelViewer) {
			// This is TheModelViewer in LCS
			LoadingScreen("Loading the ModelViewer", NULL, GetRandomSplashScreen());
			CAnimViewer::Initialise();
			CTimer::Update();
#ifndef PS2_MENU
			FrontEndMenuManager.m_bGameNotLoaded = false;
#endif
		}
#endif

#ifdef PS2_MENU
		if (TheMemoryCard.m_bWantToLoad)
			LoadSplash(GetLevelSplashScreen(CGame::currLevel));
		
		TheMemoryCard.m_bWantToLoad = false;
		
		CTimer::Update();
		
		while( !RsGlobal.quit && !(FrontEndMenuManager.m_bWantToRestart || TheMemoryCard.b_FoundRecentSavedGameWantToLoad) && !hostWindowShouldClose() )
#else
		while( !RsGlobal.quit && !FrontEndMenuManager.m_bWantToRestart && !hostWindowShouldClose())
#endif
		{
			hostPollEvents();
#ifndef MASTER
			if (gbModelViewer) {
				// This is TheModelViewerCore in LCS
				TheModelViewer();
			} else
#endif
			if ( ForegroundApp )
			{
				switch ( gGameState )
				{
					case GS_START_UP:
					{
#ifdef NO_MOVIES
						gGameState = GS_INIT_ONCE;
#else
						gGameState = GS_INIT_LOGO_MPEG;
#endif
						TRACE("gGameState = GS_INIT_ONCE");
						break;
					}

				    case GS_INIT_LOGO_MPEG:
					{
					    //if (!startupDeactivate)
						//    PlayMovieInWindow(cmdShow, "movies\\Logo.mpg");
					    gGameState = GS_LOGO_MPEG;
					    TRACE("gGameState = GS_LOGO_MPEG;");
					    break;
				    }

				    case GS_LOGO_MPEG:
					{
//					    CPad::UpdatePads();

//					    if (startupDeactivate || ControlsManager.GetJoyButtonJustDown() != 0)
						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetLeftMouseJustDown())
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetEnterJustDown())
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetCharJustDown(' '))
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetAltJustDown())
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetTabJustDown())
//						    ++gGameState;

					    break;
				    }

				    case GS_INIT_INTRO_MPEG:
					{
//#ifndef NO_MOVIES
//					    CloseClip();
//					    CoUninitialize();
//#endif
//
//					    if (CMenuManager::OS_Language == LANG_FRENCH || CMenuManager::OS_Language == LANG_GERMAN)
//						    PlayMovieInWindow(cmdShow, "movies\\GTAtitlesGER.mpg");
//					    else
//						    PlayMovieInWindow(cmdShow, "movies\\GTAtitles.mpg");

					    gGameState = GS_INTRO_MPEG;
					    TRACE("gGameState = GS_INTRO_MPEG;");
					    break;
				    }

				    case GS_INTRO_MPEG:
					{
//					    CPad::UpdatePads();
//
//					    if (startupDeactivate || ControlsManager.GetJoyButtonJustDown() != 0)
						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetLeftMouseJustDown())
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetEnterJustDown())
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetCharJustDown(' '))
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetAltJustDown())
//						    ++gGameState;
//					    else if (CPad::GetPad(0)->GetTabJustDown())
//						    ++gGameState;

					    break;
				    }

					case GS_INIT_ONCE:
					{
						//CoUninitialize();
						
#ifdef PS2_MENU
						extern char version_name[64];
						if ( CGame::frenchGame || CGame::germanGame )
							LoadingScreen(NULL, version_name, "loadsc24");
						else
							LoadingScreen(NULL, version_name, "loadsc0");
						
						printf("Into TheGame!!!\n");
#else				
						LoadingScreen(nil, nil, "loadsc0");
						// LoadingScreen(nil, nil, "loadsc0"); // duplicate
#endif
						if ( !CGame::InitialiseOnceAfterRW() )
							RsGlobal.quit = TRUE;
						
#ifdef PS2_MENU
						gGameState = GS_INIT_PLAYING_GAME;
#else
						gGameState = GS_INIT_FRONTEND;
						TRACE("gGameState = GS_INIT_FRONTEND;");
#endif
						break;
					}
#ifndef PS2_MENU
					case GS_INIT_FRONTEND:
					{
						LoadingScreen(nil, nil, "loadsc0");
						// LoadingScreen(nil, nil, "loadsc0"); // duplicate
						
						FrontEndMenuManager.m_bGameNotLoaded = true;
						
						FrontEndMenuManager.m_bStartUpFrontEndRequested = true;
						
						if ( defaultFullscreenRes )
						{
							defaultFullscreenRes = FALSE;
							FrontEndMenuManager.m_nPrefsVideoMode = GcurSelVM;
							FrontEndMenuManager.m_nDisplayVideoMode = GcurSelVM;
						}
						
						gGameState = GS_FRONTEND;
						TRACE("gGameState = GS_FRONTEND;");
						break;
					}
					
					case GS_FRONTEND:
					{
						if(!WindowIconified)
							RsEventHandler(rsFRONTENDIDLE, nil);

#ifdef PS2_MENU
						if ( !FrontEndMenuManager.m_bMenuActive || TheMemoryCard.m_bWantToLoad )
#else
						if ( !FrontEndMenuManager.m_bMenuActive || FrontEndMenuManager.m_bWantToLoad )
#endif
						{
							gGameState = GS_INIT_PLAYING_GAME;
							TRACE("gGameState = GS_INIT_PLAYING_GAME;");
						}

#ifdef PS2_MENU
						if (TheMemoryCard.m_bWantToLoad )
#else
						if ( FrontEndMenuManager.m_bWantToLoad )
#endif
						{
							InitialiseGame();
							FrontEndMenuManager.m_bGameNotLoaded = false;
							gGameState = GS_PLAYING_GAME;
							TRACE("gGameState = GS_PLAYING_GAME;");
						}
						break;
					}
#endif
					
					case GS_INIT_PLAYING_GAME:
					{
#ifdef PS2_MENU
						CGame::Initialise("DATA\\GTA3.DAT");
						
						//LoadingScreen("Starting Game", NULL, GetRandomSplashScreen());
					
						if (   TheMemoryCard.CheckCardInserted(CARD_ONE) == CMemoryCard::NO_ERR_SUCCESS
							&& TheMemoryCard.ChangeDirectory(CARD_ONE, TheMemoryCard.Cards[CARD_ONE].dir)
							&& TheMemoryCard.FindMostRecentFileName(CARD_ONE, TheMemoryCard.MostRecentFile) == true
							&& TheMemoryCard.CheckDataNotCorrupt(TheMemoryCard.MostRecentFile))
						{
							strcpy(TheMemoryCard.LoadFileName, TheMemoryCard.MostRecentFile);
							TheMemoryCard.b_FoundRecentSavedGameWantToLoad = true;
					
							if (CMenuManager::m_PrefsLanguage != TheMemoryCard.GetLanguageToLoad())
							{
								CMenuManager::m_PrefsLanguage = TheMemoryCard.GetLanguageToLoad();
								TheText.Unload();
								TheText.Load();
							}
					
							CGame::currLevel = (eLevelName)TheMemoryCard.GetLevelToLoad();
						}
#else
						InitialiseGame();

						FrontEndMenuManager.m_bGameNotLoaded = false;
#endif
						gGameState = GS_PLAYING_GAME;
						TRACE("gGameState = GS_PLAYING_GAME;");
						break;
					}
					
					case GS_PLAYING_GAME:
					{
						float ms = (float)CTimer::GetCurrentTimeInCycles() / (float)CTimer::GetCyclesPerMillisecond();
						if ( RwInitialised )
						{
							if (!FrontEndMenuManager.m_PrefsFrameLimiter || (1000.0f / (float)RsGlobal.maxFPS) < ms)
								RsEventHandler(rsIDLE, (void *)TRUE);
						}
						break;
					}
				}
			}
			else
			{
				if ( RwCameraBeginUpdate(Scene.camera) )
				{
					RwCameraEndUpdate(Scene.camera);
					ForegroundApp = TRUE;
					RsEventHandler(rsACTIVATE, (void *)TRUE);
				}
				
			}
		}

		
		/* 
		* About to shut down - block resize events again...
		*/
		RwInitialised = FALSE;
		
		FrontEndMenuManager.UnloadTextures();
#ifdef PS2_MENU	
		if ( !(FrontEndMenuManager.m_bWantToRestart || TheMemoryCard.b_FoundRecentSavedGameWantToLoad))
			break;
#else
		if ( !FrontEndMenuManager.m_bWantToRestart )
			break;
#endif
		
		CPad::ResetCheats();
		CPad::StopPadsShaking();
		
		DMAudio.ChangeMusicMode(MUSICMODE_DISABLE);
		
#ifdef PS2_MENU
		CGame::ShutDownForRestart();
#endif
		
		CTimer::Stop();
		
#ifdef PS2_MENU
		if (FrontEndMenuManager.m_bWantToRestart || TheMemoryCard.b_FoundRecentSavedGameWantToLoad)
		{
			if (TheMemoryCard.b_FoundRecentSavedGameWantToLoad)
			{
				FrontEndMenuManager.m_bWantToRestart = true;
				TheMemoryCard.m_bWantToLoad = true;
			}

			CGame::InitialiseWhenRestarting();
			DMAudio.ChangeMusicMode(MUSICMODE_GAME);
			FrontEndMenuManager.m_bWantToRestart = false;
			
			continue;
		}
		
		CGame::ShutDown();	
		CTimer::Stop();
		
		break;
#else
		if ( FrontEndMenuManager.m_bWantToLoad )
		{
			CGame::ShutDownForRestart();
			CGame::InitialiseWhenRestarting();
			DMAudio.ChangeMusicMode(MUSICMODE_GAME);
			LoadSplash(GetLevelSplashScreen(CGame::currLevel));
			FrontEndMenuManager.m_bWantToLoad = false;
		}
		else
		{
#ifndef MASTER
			if ( gbModelViewer )
				CAnimViewer::Shutdown();
			else
#endif
			if ( gGameState == GS_PLAYING_GAME )
				CGame::ShutDown();
			
			CTimer::Stop();
			
			if ( FrontEndMenuManager.m_bFirstTime == true )
			{
				gGameState = GS_INIT_FRONTEND;
				TRACE("gGameState = GS_INIT_FRONTEND;");
			}
			else
			{
				gGameState = GS_INIT_PLAYING_GAME;
				TRACE("gGameState = GS_INIT_PLAYING_GAME;");
			}
		}
		
		FrontEndMenuManager.m_bFirstTime = false;
		FrontEndMenuManager.m_bWantToRestart = false;
#endif
	}
	
#ifndef MASTER
	if ( gbModelViewer )
		CAnimViewer::Shutdown();
	else
#endif
	if ( gGameState == GS_PLAYING_GAME )
		CGame::ShutDown();

	DMAudio.Terminate();
	
	_psFreeVideoModeList();


	/*
	 * Tidy up the 3D (RenderWare) components of the application...
	 */
	RsEventHandler(rsRWTERMINATE, nil);

	/*
	 * Free the platform dependent data...
	 */
	RsEventHandler(rsTERMINATE, nil);


	return 0;
}

/*
 *****************************************************************************
 */

RwV2d leftStickPos;
RwV2d rightStickPos;

void CapturePad(RwInt32 padID)
{
	int8 hostPad = -1;

	if( padID == 0 )
		hostPad = PSGLOBAL(joy1id);
	else if( padID == 1)
		hostPad = PSGLOBAL(joy2id);
	else
		assert("invalid padID");
	
	if ( hostPad == -1 )
		return;
	
	int numButtons, numAxes;
	const uint8 *buttons = hostGetJoystickButtons(hostPad, &numButtons);
	const float *axes = hostGetJoystickAxes(hostPad, &numAxes);
	HostGamepadState gamepadState;

	if (ControlsManager.m_bFirstCapture == false) {
		memcpy(&ControlsManager.m_OldState, &ControlsManager.m_NewState, sizeof(ControlsManager.m_NewState));
	} else {
		// In case connected gamepad doesn't have L-R trigger axes.
		ControlsManager.m_NewState.mappedButtons[15] = ControlsManager.m_NewState.mappedButtons[16] = 0;
	}

	ControlsManager.m_NewState.buttons = (uint8*)buttons;
	ControlsManager.m_NewState.numButtons = numButtons;
	ControlsManager.m_NewState.id = hostPad;
	ControlsManager.m_NewState.isGamepad = hostGetGamepadState(hostPad, &gamepadState);
	if (ControlsManager.m_NewState.isGamepad) {
		memcpy(&ControlsManager.m_NewState.mappedButtons, gamepadState.buttons, sizeof(gamepadState.buttons));
		float lt = gamepadState.axes[HOST_PAD_AXIS_LEFT_TRIGGER], rt = gamepadState.axes[HOST_PAD_AXIS_RIGHT_TRIGGER];

		// the host (like glfw) returns 0.0 for non-existent axes(which is bullocks) so we treat it as deadzone, and keep value of previous frame.
		// otherwise if this axis is present, -1 = released, 1 = pressed
		if (lt != 0.0f)
			ControlsManager.m_NewState.mappedButtons[15] = lt > -0.8f;

		if (rt != 0.0f)
			ControlsManager.m_NewState.mappedButtons[16] = rt > -0.8f;
	}
	// TODO? L2-R2 axes(not buttons-that's fine) on joysticks that don't have SDL gamepad mapping AREN'T handled, and I think it's impossible to do without mapping.

	if (ControlsManager.m_bFirstCapture == true) {
		memcpy(&ControlsManager.m_OldState, &ControlsManager.m_NewState, sizeof(ControlsManager.m_NewState));
		
		ControlsManager.m_bFirstCapture = false;
	}

	RsPadButtonStatus bs;
	bs.padID = padID;

	RsPadEventHandler(rsPADBUTTONUP, (void *)&bs);
	
	// Gamepad axes are guaranteed to return 0.0f if that particular gamepad doesn't have that axis.
	// And that's really good for sticks, because gamepads return 0.0 for them when sticks are in released state.
	if ( hostPad != -1 ) {
		leftStickPos.x = ControlsManager.m_NewState.isGamepad ? gamepadState.axes[HOST_PAD_AXIS_LEFT_X] : numAxes >= 1 ? axes[0] : 0.0f;
		leftStickPos.y = ControlsManager.m_NewState.isGamepad ? gamepadState.axes[HOST_PAD_AXIS_LEFT_Y] : numAxes >= 2 ? axes[1] : 0.0f;

		rightStickPos.x = ControlsManager.m_NewState.isGamepad ? gamepadState.axes[HOST_PAD_AXIS_RIGHT_X] : numAxes >= 3 ? axes[2] : 0.0f;
		rightStickPos.y = ControlsManager.m_NewState.isGamepad ? gamepadState.axes[HOST_PAD_AXIS_RIGHT_Y] : numAxes >= 4 ? axes[3] : 0.0f;
	}
	
	{
		if (CPad::m_bMapPadOneToPadTwo)
			bs.padID = 1;
		
		RsPadEventHandler(rsPADBUTTONUP,   (void *)&bs);
		RsPadEventHandler(rsPADBUTTONDOWN, (void *)&bs);
	}
	
	{
		if (CPad::m_bMapPadOneToPadTwo)
			bs.padID = 1;
		
		CPad *pad = CPad::GetPad(bs.padID);

		if ( Abs(leftStickPos.x)  > 0.3f )
			pad->PCTempJoyState.LeftStickX	= (int32)(leftStickPos.x  * 128.0f);
		
		if ( Abs(leftStickPos.y)  > 0.3f )
			pad->PCTempJoyState.LeftStickY	= (int32)(leftStickPos.y  * 128.0f);
		
		if ( Abs(rightStickPos.x) > 0.3f )
			pad->PCTempJoyState.RightStickX = (int32)(rightStickPos.x * 128.0f);

		if ( Abs(rightStickPos.y) > 0.3f )
			pad->PCTempJoyState.RightStickY = (int32)(rightStickPos.y * 128.0f);
	}

	_psHandleVibration();

	return;
}

extern "C" void revc_on_joystick(int jid, int connected)
{
	if (!HostCallbacksEnabled)
		return;
	if (connected && !IsThisJoystickBlacklisted(jid)) {
		if (PSGLOBAL(joy1id) == -1) {
			PSGLOBAL(joy1id) = jid;
#ifdef DETECT_JOYSTICK_MENU
			strcpy(gSelectedJoystickName, hostGetJoystickName(jid));
#endif
			// This is behind LOAD_INI_SETTINGS, because otherwise the Init call below will destroy/overwrite your bindings.
#ifdef LOAD_INI_SETTINGS
			int count;
			hostGetJoystickButtons(PSGLOBAL(joy1id), &count);
			ControlsManager.InitDefaultControlConfigJoyPad(count);
#endif
		} else if (PSGLOBAL(joy2id) == -1)
			PSGLOBAL(joy2id) = jid;

	} else if (!connected) {
		if (PSGLOBAL(joy1id) == jid) {
			PSGLOBAL(joy1id) = -1;
		} else if (PSGLOBAL(joy2id) == jid)
			PSGLOBAL(joy2id) = -1;
	}
}

#endif // RW_GL3 && LIBRW_HOST
