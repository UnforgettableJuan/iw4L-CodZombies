# Zombies: World at War, Black Ops, Black Ops II

Goal: play every retail zombies map from the three Treyarch games on this engine,
read from installations the player owns. No map, script, model or sound from those
games is ever committed or shipped; the same rule as MW2 ([`CONTRIBUTING.md`](../CONTRIBUTING.md)).

## What exists

`asset_transport::zombies` holds the catalog (`ZOMBIES_MAPS`, 21 maps by zone stem)
and finds them under `IW4L_GAMES`. The launcher logs one `zombies <game>: n/m maps
found` line per game, then each present map with its `make map` key when a reader
exists. A stem whose envelope belongs to another game is reported, not loaded.

| game | envelope | reader | maps |
|---|---|---|---|
| World at War | `IWffu100` 0x183 | none | `nazi_zombie_{prototype,asylum,sumpf,factory}` |
| Black Ops | `IWff0100` 0x1D9 | `fastfile_t5` | `zombie_*`, `zombie_cod5_*` (Rezurrection remasters of the four WaW maps) |
| Black Ops II | `TAff0100` 0x93 | none | `zm_{transit,transit_dr,nuked,highrise,prison,buried,tomb}` |

Black Ops is first because the T5 reader already loads its multiplayer maps, and
its `zombie_cod5_*` zones carry all four World at War maps.

## Phases

1. **Discovery** (done): catalog, envelope check, launch report.
2. **Walk Nacht.** `make map t5:zombie_cod5_prototype` reaches a free-roam world:
   the walk uses BO1's zombies common zones in place of `common_mp`, keeps map
   entities, and needs no gameplay. Which common zones a zombies map depends on is
   read from a real install, not guessed.
3. **Zombies mode.** Rounds, points, doors and debris, wall buys, the box, perks
   and power, driven by the map's entities. MW2 has no AI actors, so zombies start
   as host-simulated entities on the bot navigation grid with T5 anims. Running
   Treyarch's own `_zombiemode` GSC would need the full actor and AI script stack;
   that is a later option, not the first step.
4. **Rest of Black Ops**: Kino, Five, Ascension, Call of the Dead, Shangri-La, Moon,
   one map per slice, each adding the mechanics it introduces.
5. **World at War reader** (`fastfile_t4`): close to T5, for players without the
   Rezurrection remasters.
6. **Black Ops II reader** (`fastfile_t6`): encrypted, signed `TAff` zones and
   Direct3D 11 shader bytecode, so a new shader translator beside `d3d9_sm3`.
   The largest step; then TranZit, Die Rise, Mob of the Dead, Buried, Origins.

Each phase is its own pull request and is checked against a real install before
the next starts.
