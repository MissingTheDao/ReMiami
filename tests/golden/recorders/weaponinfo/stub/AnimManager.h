#pragma once
// Recorder stand-in: the game's anim group names (AnimManager.cpp ms_aAnimAssocDefinitions).
typedef int AssocGroupId;
enum { ASSOCGRP_UNARMED = 7 };
static const char *const kAnimGroupNames[] = { "man","van","coach","bikes","bikev","bikeh","biked","unarmed","screwdrv","knife","baseball","golfclub","chainsaw","python","colt45","shotgun","buddy","tec","uzi","rifle","m60","sniper","grenade","flame","medic","sunbathe","playidles","riot","strip","lance","player","playerrocket","player1armed","player2armed","playerBBBat","playercsaw","shuffle","oldman","gang1","gang2","fatman","oldfatman","jogger","woman","shopping","busywoman","sexywoman","fatwoman","oldwoman","jogwoman","panicchunky","skate","playerback","playerleft","playerright","rocketback","rocketleft","rocketright","csawback","csawleft","csawright" };
enum { NUM_ANIM_ASSOC_GROUPS = sizeof(kAnimGroupNames) / sizeof(kAnimGroupNames[0]) };
struct CAnimManager { static const char *GetAnimGroupName(AssocGroupId i) { return kAnimGroupNames[i]; } };
