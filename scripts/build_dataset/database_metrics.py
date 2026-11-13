# -*- coding: utf-8 -*-
"""Métriques de la base interne de FragHub, pour le README : nombres de molécules, champs remplis, et sunbursts des
deux ontologies (ClassyFire, NPClassifier), avec et sans les classes inconnues.

Usage (à la racine du dépôt, avec la base installée dans datas/) :
    python scripts/build_dataset/database_metrics.py [--base datas/internal_databases.parquet] [--no-png]

Écrit dans documentation/internal_database/ :
- metrics.json : les nombres (repris dans le README) ;
- internal_database_metrics.html : page interactive (Plotly, chargé depuis cdn.plot.ly) ;
- sunburst_classyfire.png, sunburst_classyfire_known.png, sunburst_npclassifier.png,
  sunburst_npclassifier_known.png : images pour le README (GitHub n'affiche pas de page interactive). Les images
  demandent `plotly` et `kaleido` (pip install plotly kaleido) ; sans eux, ou avec --no-png, elles ne sont pas faites.

Classe inconnue : valeur vide ou « NOT FOUND ». Les classifications multiples de NPClassifier (« Alkaloids|Terpenoids »)
sont gardées telles qu'elles sont dans la base, comme une classe à part.
"""
import argparse
import collections
import glob
import json
import os
import sys

import pyarrow.parquet as pq

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "documentation", "internal_database")
UNKNOWN = "Unknown"
ONTOLOGIES = {
    "classyfire": ("ClassyFire", ["CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS"],
                   ["Superclass", "Class", "Subclass"]),
    "npclassifier": ("NPClassifier", ["NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS"],
                     ["Pathway", "Superclass", "Class"]),
}
FIELDS = ["INCHI", "SMILES", "FORMULA", "NAME", "EXACTMASS"]
PLOTLY = "https://cdn.plot.ly/plotly-2.27.0.min.js"
# Palette qualitative sobre (Tableau 10, lisible sur fond blanc) ; les niveaux inférieurs prennent des teintes plus
# claires de la couleur de leur premier niveau, pour que la hiérarchie se lise d'un coup d'œil
COLORWAY = ["#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1", "#ff9da7", "#9c755f",
            "#bab0ac"]
UNKNOWN_COLOR = "#e4e4e4"
TINTS = {1: (0.0, 0.0), 2: (0.30, 0.45), 3: (0.55, 0.68)}   # part de blanc par niveau, alternée entre voisins


def tint(color, white):
    """Mélange une couleur #rrggbb avec du blanc (white = 0 : couleur pure, 1 : blanc)."""
    rgb = [int(color[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join("%02x" % round(c + (255 - c) * white) for c in rgb)


def clean(value):
    """Classe lisible, ou UNKNOWN pour une valeur vide ou « NOT FOUND »."""
    if value is None:
        return UNKNOWN
    v = str(value).strip()
    return UNKNOWN if not v or v.upper() in ("NOT FOUND", "NAN", "NONE") else v.replace("|", " | ")


def count(base):
    files = sorted(glob.glob(os.path.join(base, "*.parquet"))) if os.path.isdir(base) else [base]
    if not files:
        sys.exit("Base interne introuvable : %s (python scripts/internal_database.py)" % base)
    total, filled = 0, collections.Counter()
    paths = {key: collections.Counter() for key in ONTOLOGIES}
    for f in files:
        cols = [c for _, cs, _ in ONTOLOGIES.values() for c in cs]
        t = pq.read_table(f, columns=cols + FIELDS)
        total += t.num_rows
        for c in FIELDS:
            filled[c] += t.num_rows - t.column(c).null_count
        for key, (_, cs, _) in ONTOLOGIES.items():
            levels = [t.column(c).to_pylist() for c in cs]
            paths[key].update(tuple(clean(v) for v in p) for p in zip(*levels))
    return total, filled, paths


def sunburst(paths, keep_unknown):
    """Données Plotly (ids, labels, parents, values) d'un sunburst à trois niveaux.

    Sans les inconnus : seules les molécules sans aucune classe (inconnues aux trois niveaux) sont retirées. Une
    molécule classée à un niveau au moins garde toutes ses cases, « Unknown » compris aux autres niveaux."""
    values = collections.Counter()
    for path, n in paths.items():
        if not keep_unknown and all(c == UNKNOWN for c in path):
            continue
        for depth in range(1, 4):
            values[path[:depth]] += n
    ids, labels, parents, vals, colors = [], [], [], [], []
    tops = sorted({p[0] for p in values if len(p) == 1}, key=lambda k: -values[(k,)])
    top_color = {}
    i = 0
    for t in tops:
        if t == UNKNOWN:
            top_color[t] = UNKNOWN_COLOR
        else:
            top_color[t] = COLORWAY[i % len(COLORWAY)]
            i += 1
    rank = collections.Counter()   # rang de chaque case parmi ses sœurs, pour alterner les teintes
    for path in sorted(values, key=lambda p: (len(p), -values[p])):
        ids.append(" / ".join(path))
        labels.append(path[-1])
        parents.append(" / ".join(path[:-1]))
        vals.append(values[path])
        if path[-1] == UNKNOWN or path[0] == UNKNOWN:
            colors.append(UNKNOWN_COLOR)
        else:
            colors.append(tint(top_color[path[0]], TINTS[len(path)][rank[path[:-1]] % 2]))
        rank[path[:-1]] += 1
    return {"ids": ids, "labels": labels, "parents": parents, "values": vals, "colors": colors}


def summary(total, filled, paths):
    out = {"molecules": total, "fields": {c: filled[c] for c in FIELDS}, "ontologies": {}}
    for key, (title, cs, names) in ONTOLOGIES.items():
        levels = []
        for d in range(3):
            known = sum(n for p, n in paths[key].items() if p[d] != UNKNOWN)
            classes = len({p[d] for p in paths[key] if p[d] != UNKNOWN})
            levels.append({"level": names[d], "column": cs[d], "classified": known, "classes": classes})
        out["ontologies"][key] = {"title": title, "levels": levels}
    return out


def pct(n, total):
    return "%.1f %%" % (100.0 * n / total) if total else "-"


def html_page(meta, charts):
    m = meta["summary"]
    rows = "".join(
        "<tr><td>%s</td><td>%s</td><td>%s</td><td>%s</td></tr>" % (
            o["title"], lv["level"], "{:,}".format(lv["classified"]).replace(",", " "), pct(lv["classified"], m["molecules"]))
        for o in m["ontologies"].values() for lv in o["levels"])
    data = json.dumps(charts, ensure_ascii=False, separators=(",", ":"))
    return """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>FragHub internal database</title>
<script src="%(plotly)s"></script>
<style>
body { margin:0; background:#ffffff; color:#222; font:15px/1.5 "Helvetica Neue", Helvetica, Arial, sans-serif; }
main { max-width:1000px; margin:0 auto; padding:28px 16px 48px; }
h1 { font-size:1.5rem; font-weight:600; margin:0 0 2px; } .sub { color:#666; margin:0 0 22px; }
a { color:#1f5f9e; }
.stats { display:grid; grid-template-columns:repeat(auto-fit, minmax(150px, 1fr)); border-top:2px solid #222; border-bottom:1px solid #222; margin-bottom:26px; }
.stat { padding:10px 12px; } .stat + .stat { border-left:1px solid #ddd; }
.stat b { display:block; font-size:1.3rem; font-weight:600; font-variant-numeric:tabular-nums; } .stat span { color:#666; font-size:.82rem; }
table { border-collapse:collapse; width:100%%; margin-bottom:30px; font-size:.9rem; border-top:2px solid #222; border-bottom:2px solid #222; }
caption { caption-side:top; text-align:left; font-weight:600; padding-bottom:6px; }
th { border-bottom:1px solid #222; font-weight:600; } th, td { padding:5px 10px; text-align:left; }
th:nth-child(n+3), td:nth-child(n+3) { text-align:right; font-variant-numeric:tabular-nums; }
tbody tr:nth-child(3) td { border-bottom:1px solid #ccc; }
.controls { display:flex; flex-wrap:wrap; gap:14px; align-items:center; margin-bottom:6px; }
.seg { display:inline-flex; border:1px solid #888; border-radius:4px; overflow:hidden; }
.seg button { font:inherit; font-size:.9rem; border:0; background:#fff; color:#222; padding:5px 14px; cursor:pointer; }
.seg button + button { border-left:1px solid #888; }
.seg button[aria-pressed="true"] { background:#222; color:#fff; }
#chart { width:100%%; height:min(82vh, 780px); }
.note { color:#555; font-size:.85rem; border-top:1px solid #ccc; padding-top:8px; }
</style></head>
<body><main>
<h1>FragHub internal database</h1>
<p class="sub">Version %(version)s &middot; <a href="https://doi.org/%(doi)s">doi:%(doi)s</a> (Zenodo)</p>
<div class="stats">
  <div class="stat"><b>%(molecules)s</b><span>molecules (InChIKey)</span></div>
  <div class="stat"><b>%(structures)s</b><span>with a structure (InChI, SMILES)</span></div>
  <div class="stat"><b>%(names)s</b><span>with a name</span></div>
  <div class="stat"><b>%(cf)s</b><span>with a ClassyFire superclass</span></div>
  <div class="stat"><b>%(np)s</b><span>with an NPClassifier pathway</span></div>
</div>
<table><caption>Classified molecules per ontology level</caption><thead><tr><th>Ontology</th><th>Level</th><th>Molecules classified</th><th>Share</th></tr></thead><tbody>%(rows)s</tbody></table>
<div class="controls" role="toolbar">
  <span class="seg"><button data-o="classyfire" aria-pressed="true">ClassyFire</button><button data-o="npclassifier" aria-pressed="false">NPClassifier</button></span>
  <span class="seg"><button data-u="1" aria-pressed="true">With unknown</button><button data-u="0" aria-pressed="false">Without unknown</button></span>
</div>
<div id="chart"></div>
<p class="note">Click a sector to zoom in, click the centre to zoom out. Two levels are shown at a time. &laquo;&nbsp;Unknown&nbsp;&raquo;: no class in the database (empty or NOT FOUND). Multiple NPClassifier classifications (&laquo;&nbsp;Alkaloids | Terpenoids&nbsp;&raquo;) are kept as written in the database. Without unknown, only the molecules with no class at any level are left out; the others keep their &laquo;&nbsp;Unknown&nbsp;&raquo; sectors at the other levels.</p>
</main>
<script>
const CHARTS = %(data)s;
let ontology = 'classyfire', unknown = '1';
function draw() {
  const c = CHARTS[ontology + (unknown === '1' ? '' : '_known')];
  Plotly.react('chart', [{
    type: 'sunburst', ids: c.ids, labels: c.labels, parents: c.parents, values: c.values,
    branchvalues: 'total', maxdepth: 2, insidetextorientation: 'radial', sort: false,
    marker: { colors: c.colors, line: { color: '#ffffff', width: 0.8 } },
    hovertemplate: '<b>%%{label}</b><br>%%{value:,} molecules<br>%%{percentRoot:.1%%} of the chart<br>%%{percentParent:.1%%} of %%{parent}<extra></extra>'
  }], { margin: { t: 10, l: 0, r: 0, b: 10 }, paper_bgcolor: '#ffffff', font: { family: 'Helvetica Neue, Helvetica, Arial, sans-serif', color: '#222', size: 12 },
    hoverlabel: { bgcolor: '#ffffff', bordercolor: '#888', font: { color: '#222' } } },
  { displaylogo: false, responsive: true });
}
document.querySelectorAll('.controls button').forEach((b) => b.addEventListener('click', () => {
  if (b.dataset.o) { ontology = b.dataset.o; document.querySelectorAll('[data-o]').forEach((x) => x.setAttribute('aria-pressed', x === b)); }
  if (b.dataset.u) { unknown = b.dataset.u; document.querySelectorAll('[data-u]').forEach((x) => x.setAttribute('aria-pressed', x === b)); }
  draw();
}));
draw();
</script></body></html>
""" % {
        "plotly": PLOTLY, "version": meta["version"], "doi": meta["doi"],
        "molecules": "{:,}".format(m["molecules"]).replace(",", " "),
        "structures": "{:,}".format(m["fields"]["INCHI"]).replace(",", " "),
        "names": "{:,}".format(m["fields"]["NAME"]).replace(",", " "),
        "cf": "{:,}".format(m["ontologies"]["classyfire"]["levels"][0]["classified"]).replace(",", " "),
        "np": "{:,}".format(m["ontologies"]["npclassifier"]["levels"][0]["classified"]).replace(",", " "),
        "rows": rows, "data": data}


def pngs(charts, meta):
    try:
        import plotly.graph_objects as go
    except ImportError:
        print("plotly absent : pas d'images (pip install plotly kaleido)")
        return
    for name, c in charts.items():
        ontology = "ClassyFire" if name.startswith("classyfire") else "NPClassifier"
        title = ontology + (" (without unknown)" if name.endswith("_known") else " (with unknown)")
        fig = go.Figure(go.Sunburst(ids=c["ids"], labels=c["labels"], parents=c["parents"], values=c["values"],
                                    branchvalues="total", maxdepth=2, insidetextorientation="radial", sort=False,
                                    marker=dict(colors=c["colors"], line=dict(color="#ffffff", width=0.8))))
        fig.update_layout(title=dict(text="%s · FragHub internal database %s" % (title, meta["version"]), x=0.5,
                                     font=dict(size=18)),
                          margin=dict(t=60, l=10, r=10, b=10), paper_bgcolor="#ffffff",
                          font=dict(family="Helvetica, Arial, sans-serif", size=12, color="#222222"))
        path = os.path.join(OUT, "sunburst_%s.png" % name)
        try:
            fig.write_image(path, width=900, height=900, scale=2)
        except Exception as e:  # noqa: BLE001 — kaleido ou Chrome absent
            print("image non faite (%s) : %s" % (path, e))
            return
        print("écrit :", os.path.relpath(path, ROOT))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base", default=os.path.join(ROOT, "datas", "internal_databases.parquet"))
    ap.add_argument("--no-png", action="store_true")
    args = ap.parse_args()
    with open(os.path.join(ROOT, "datas", "internal_database.json"), encoding="utf-8") as f:
        ref = json.load(f)
    total, filled, paths = count(args.base)
    charts = {}
    for key in ONTOLOGIES:
        charts[key] = sunburst(paths[key], True)
        charts[key + "_known"] = sunburst(paths[key], False)
    meta = {"version": ref["version"], "doi": ref["doi"], "summary": summary(total, filled, paths)}
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "metrics.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)   # les données des sunbursts sont dans la page HTML
    with open(os.path.join(OUT, "internal_database_metrics.html"), "w", encoding="utf-8", newline="\n") as f:
        f.write(html_page(meta, charts))
    print(json.dumps(meta["summary"], indent=1))
    if not args.no_png:
        pngs(charts, meta)


if __name__ == "__main__":
    main()
