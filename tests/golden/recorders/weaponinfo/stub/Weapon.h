#pragma once
struct CWeapon { static bool IsShotgun(int weapon) { return weapon == WEAPONTYPE_SHOTGUN || weapon == WEAPONTYPE_SPAS12_SHOTGUN || weapon == WEAPONTYPE_STUBBY_SHOTGUN; } };
