# Gameplay/feature plans

#### Meta progression
- 

#### Extra replayability
- add new biomes/bosses and randomly order the biomes in the world (besides hub. but, possibly add a Hub 2.0)

**World shuffle**
- DONE (system): `level.arrange` deals biomes onto the grid at launch (debug flags `seed`, `fixed_map`). Places (gates, open bits, shrines, minimap) stay keyed by slot; what a slot is comes from `level.biomeAt(slot)` (name/look in `biome.j3`, boss in `boss.j3` `bossOf`/`biomeOf`). Hub and Volcano are pinned (the crater is the Hollows shaft).
- to add a biome: draw it on the staging strip (x = 1000 + (id - 10) * 200, z 0, ids 10+), add it to `level.pooled` and bump `POOL`, add name/look/minimap colour/boss. Pool bigger than the 7 free slots means some biomes sit a run out (victory only needs the bosses on the map).
- not yet: reshuffle on restart (needs a scene reload), Hub 2.0, seed on the death screen.
- but, volcano and crystal hollows always sit in the same corner spot. 
- DONE Hub 2.0: the Spires (biome id 10, `spires` in arena.j3s) takes slot 4 on half the seeds. A tall core with a spiral up to a deck, four piers with pads, corner beacons. Force one with debug `hub = 1` (classic) or `2` (Spires). New pool ids now start at 11.

**New biomes** 
- Moon Base: SCRAPPED (built and removed). Its Saucer boss idea is free again.
    - Features: lunar base theme, slightly reduced gravity, a modular base that you can actually enter. the center of the moon base is a large dome viewing room (everything rectangular shaped)
    - Enemy: little research rovers that chase the player
    - Boss: Saucer (add a new boss for The Isles)
- The Beach: (id 11, `beach` in arena.j3s; debug `biome_on = true`, `biome = 11`) DONE: sand, a 140x90 sea with stepped seabed (see-through `water` material, `DecoWater...` objects are swim volumes: wading slows, deep water swims, underwater fog + screen tint), dock, sandbar island, reef + kelp, tiki hut, condo with pool, lifeguard tower, volleyball, fire pit. Enemies DONE: crabs (keep to sand, lunge) and puffers (spawn under water, swell and fire spine fans; both zone enemies of the beach). Boss DONE: Stormeel (`stormeel.j3`, boss id 10, debug boss name `stormeel`/`eel`): a flying worm circling the player, high (calls lightning) and low (skims the ground/waves, bites, glowing segments open). Not run in-game yet, tune numbers
    - Features: a beach with a tiki hut and condo. most of the biome is the beach lowering into waters, those waters lowering into a coral reef and sunken ship.
    - Enemy: puffer fish confined to water, crabs confined to beach/land
    - Boss: Massive electric eel with thunder attacks
- The Swamp: (id 12, `swamp` in arena.j3s; debug `biome_on = true`, `biome = 12`) DONE: 4 shallow ponds (wade, slowed) each with a plank bridge, dark swamp trees with moss, big boxy bouncy mushrooms (`JumpPad26_` caps), stilt shack, lily pads, cattails, logs. Enemy DONE: leaping frog (`FROG`, cute, long arcs). Boss DONE: Bullfrog (`bullfrog.j3`, boss id 11, debug `bullfrog`/`frog`): leaps onto marked rings, croaks (swollen throat = weak spot) spitting bubbles, headbutts if you hug it. Not run in-game yet, tune numbers
    - Features: a swamp with dark swamp trees, bridges over the waters, and waters themselves that slow you down. big boxy mushrooms that are bouncy.
    - Enemy: leaping frogs
    - Boss: Massive frog that jumps, throws bubbles, and headbutts the player if too close
- The Graveyard: (id 13, `graveyard` in arena.j3s; debug `biome_on = true`, `biome = 13`) DONE: enterable stone tomb (sarcophagi, altar, torches, stairs to a walkable stepped roof), two fields of headstones, small crypts, dead trees, iron fence + gate, lamp-lit path, pumpkins, ground fog (`DecoFog`, water material). Enemy DONE: flying skull (circles, then dives). Boss DONE: Bonegolem (`bonegolem.j3`, boss id 12, debug `bonegolem`/`golem`): flaming skull volleys, bone pillars from the ground (warning rings), roars when you hug it; soul core in the ribs is the weak spot. Not run in-game yet, tune numbers
    - Features: a graveyard next to a large tomb that you can enter. some ground fog.
    - Enemy: flying skulls
    - Boss: chunky skeleton golem, shoots flaming skulls, summons bones that shoot out of the ground, and roar when the player is too close
- The Farm (DONE, id 14; not run in-game yet, tune numbers):
    - Features: corn fields who's tall corn naturally restrict visibility. a large barn and a silo that can be entered.
    - Enemy: chickens that chase you down
    - Boss: scarecrow golem. throws explosive corn bombs and is similar to colossus

**Boss ideas**
- twin bosses that share a health bar and buff each other when one falls.
- a boss that steals your current weapon and uses it back on you.
- a mirror duel: your own loadout, your own skills, a little tougher.
- a "no weak point" boss that's only damageable while you stand on its arena runes.
- rematch tiers: beating a boss unlocks Hard versions with an extra phase and a new attack.

**Run modifiers** (pick 1-2 at the title screen, higher score multiplier)
- Low Gravity, Fog of War (minimap off), One Weapon (random each level), Double Boss, Vampiric (heal only from kills, no health cards), Speedrun (timer, bosses give time back).
- Mutators that swap each level: enemies explode, enemies fast, no reloading, ricochet shots.

**Challenge modes**
- Boss Rush variants: random boss order, Hard only, no-hit ("one hit and it's over"), every boss at once.
- Gauntlet: a fixed arena, endless waves, one card every 5 waves, leaderboard on wave reached.
- Pacifist Dash: reach the far side of the map through enemies with no weapons, only skills.
- Daily Challenge: fixed seed + fixed modifier set.

**Build variety**
- weapon upgrade trees (item 10 in the old plan): each gun gets 2-3 branches that change how it plays (RPG cluster vs. laser guided).
- synergy tags on perks and gifts (fire, frost, shock, poison); collecting 3+ of a tag unlocks a bonus effect.
- cursed items: strong upside plus a real downside, offered at shrines next to the normal gift.
- class starts: Gunner, Bruiser, Mage, Trickster, each with a unique starting skill and perk pool.

**Long-term hooks**
- achievements that unlock cosmetics (weapon skins, tracer colours, hit sounds).
- run-end unlocks (item 17 in the old plan): a stat-based unlock list, e.g. "beat Rime without dashing" unlocks a new gift.
- a bestiary that fills as you meet enemies and bosses, with a small permanent bonus per full page.


#### Fixes
- 
