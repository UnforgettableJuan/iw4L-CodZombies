# Zombies: the fork's goal and where it stands

One runtime that plays Call of Duty Zombies maps from the games you own.
Black Ops (t5) first: every BO1 map, including the Rezurrection remakes of the
four World at War maps, which removes the need for a WaW reader. Black Ops II
(t6) later: TranZit, Town and Bus Depot (one zone), Nuketown Zombies, Die Rise,
Buried and Origins.

Expected BO1 zone names, to be confirmed by `--list` on a real install:
`zombie_theater`, `zombie_pentagon`, `zombie_cosmodrome`, `zombie_coast`,
`zombie_temple`, `zombie_moon`, `zombietron`, `zombie_cod5_prototype`,
`zombie_cod5_asylum`, `zombie_cod5_sumpf`, `zombie_cod5_factory`.

## First step: inventory your zones

```bash
iw4l inspect-zone --list zombie                 # every matching .ff, game, version
iw4l inspect-zone t5:zombie_theater             # one zone by name
iw4l inspect-zone 't5:zombie_*' --names         # every BO1 zombies zone
iw4l inspect-zone "D:/Games/Black Ops/zone/Common/zombie_moon.ff"   # a path
make inspect-zone ZONE='t5:zombie_*'            # same, from the repo
```

Names need `IW4L_GAMES` (the folder holding your game folders); a path does
not. Nothing opens a window or touches the GPU. Per zone it prints the asset
list by type (inline, shared, null, walked), the first asset type the walk
cannot parse, where and why the walk stopped, the world parts it reached, and
a classname histogram of the map entities. A full report with asset and raw
file names lands in `iw4l-artifacts/inspect/`. Exit 0 means every walk
completed, 1 that one stopped or a zone could not be read, 2 bad arguments.

The t5 walk has no loader yet for eleven asset types (`aitype`, `character`,
`xmodelalias`, `weapondef`, `weaponvariant`, `menu`, `ui_map`, `mptype`,
`mpbody`, `mphead`, `packindex`) and stops at the first one stored inline.
The inventory says which of them each zombies zone needs.

## Roadmap

1. Foundation: CI, unit tests allowed, `inspect-zone`. **Done.**
2. Walk BO1 zombies zones to the end, load one (Kino der Toten) and walk it.
3. A `zm` mode written in our own GSC: rounds, points, doors, wall buys, box,
   perks, power, Pack-a-Punch, downs and revives.
4. Zombie AI as a new entity type: animations, pathnodes, barricades, 24 live.
5. Per-map features across the BO1 list.
6. Black Ops II: encrypted zones, D3D11 shaders, compiled GSC.
