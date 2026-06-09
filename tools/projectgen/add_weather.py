"""Iteration 13 content: showcase the dynamic weather system.

All gameplay zones ship with WeatherChance [0,0,0,0,0] = permanently clear. The
engine picks weather every Rand(2500,10000) ticks from these 5 per-type weight
percentages (UpdateWeather, ServerAreas.bb): index i -> weather type i+1, where
1=Rain 2=Snow 3=Fog 4=Storm 5=Wind (Environment.bb); remainder of 100 = clear.

Set thematically-fitting weather per zone (sums kept well under 100 so clear
still dominates). Surgical: only WeatherChance changes; everything else stays
byte-identical. Idempotent (skips if already set to the target).
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREAS = os.path.join(DATA, 'Server Data', 'Areas')

# area -> WeatherChance[5]  (idx0=Rain,1=Snow,2=Fog,3=Storm,4=Wind)
PLAN = {
    'Plains':          [20, 0, 0, 5, 0],   # temperate: showers, rare storm
    'Test Zone':       [22, 0, 8, 0, 0],   # wilds: rain + occasional fog
    'Northern Shrine': [0, 30, 0, 0, 0],   # cold north: snow
}

def main():
    for area_name, wc in PLAN.items():
        path = os.path.join(AREAS, area_name + '.dat')
        raw = open(path, 'rb').read()
        area = rcdata.read_server_area(raw)
        if area['weather_chance'] == wc:
            print(f"  skip ({area_name}): already {wc}"); continue
        old = list(area['weather_chance'])
        area['weather_chance'] = list(wc)
        out = rcdata.write_server_area(area)
        # surgical: re-parse and confirm ONLY weather_chance differs
        chk = rcdata.read_server_area(out)
        for k in area:
            if k == 'weather_chance':
                continue
            assert chk[k] == area[k], f"section '{k}' changed in {area_name}"
        # the rest of the file (everything after the 5 weather bytes) must be identical
        assert out[5:] == raw[5:], f"bytes past weather header changed in {area_name}"
        with open(path + '.tmp', 'wb') as f:
            f.write(out)
        os.replace(path + '.tmp', path)
        print(f"  {area_name}: WeatherChance {old} -> {wc}")
    return 0

if __name__ == '__main__':
    sys.exit(main())
