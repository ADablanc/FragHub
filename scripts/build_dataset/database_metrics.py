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
COLORWAY = ["#2563eb", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#ec4899", "#06b6d4", "#f97316", "#84cc16",
            "#6366f1", "#14b8a6", "#eab308", "#a855f7", "#0ea5e9", "#22c55e", "#e11d48"]
UNKNOWN_COLOR = "#cbd5e1"


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

    Sans les inconnus : les molécules dont le premier niveau est inconnu sont retirées, et un niveau inconnu plus bas
    n'a pas de case (la case du niveau au-dessus garde son total : la part non classée reste visible en creux)."""
    values = collections.Counter()
    for path, n in paths.items():
        if not keep_unknown and path[0] == UNKNOWN:
            continue
        for depth in range(1, 4):
            sub = path[:depth]
            if not keep_unknown and sub[-1] == UNKNOWN:
                break
            values[sub] += n
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
    for path in sorted(values, key=lambda p: (len(p), -values[p])):
        ids.append(" / ".join(path))
        labels.append(path[-1])
        parents.append(" / ".join(path[:-1]))
        vals.append(values[path])
        colors.append(UNKNOWN_COLOR if path[-1] == UNKNOWN else top_color[path[0]])
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
:root { --bg:#ffffff; --fg:#1e293b; --muted:#64748b; --line:#e2e8f0; --card:#f8fafc; --accent:#2563eb; }
@media (prefers-color-scheme: dark) { :root { --bg:#0f172a; --fg:#e2e8f0; --muted:#94a3b8; --line:#334155; --card:#1e293b; --accent:#60a5fa; } }
body { margin:0; background:var(--bg); color:var(--fg); font:15px/1.5 -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; }
main { max-width:1100px; margin:0 auto; padding:24px 16px 48px; }
h1 { font-size:1.6rem; margin:0 0 4px; } .sub { color:var(--muted); margin:0 0 24px; }
a { color:var(--accent); }
.stats { display:grid; grid-template-columns:repeat(auto-fit, minmax(160px, 1fr)); gap:12px; margin-bottom:24px; }
.stat { background:var(--card); border:1px solid var(--line); border-radius:10px; padding:12px 14px; }
.stat b { display:block; font-size:1.35rem; } .stat span { color:var(--muted); font-size:.85rem; }
table { border-collapse:collapse; width:100%%; margin-bottom:28px; font-size:.9rem; }
th, td { border-bottom:1px solid var(--line); padding:6px 8px; text-align:left; } td:nth-child(n+3) { text-align:right; font-variant-numeric:tabular-nums; }
.controls { display:flex; flex-wrap:wrap; gap:8px; align-items:center; margin-bottom:8px; }
.controls button { font:inherit; border:1px solid var(--line); background:var(--card); color:var(--fg); border-radius:999px; padding:6px 14px; cursor:pointer; }
.controls button[aria-pressed="true"] { background:var(--accent); border-color:var(--accent); color:#fff; }
.controls .sep { width:1px; height:24px; background:var(--line); margin:0 4px; }
#chart { width:100%%; height:min(80vh, 760px); }
.note { color:var(--muted); font-size:.85rem; }
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
<table><thead><tr><th>Ontology</th><th>Level</th><th>Molecules classified</th><th>Share</th></tr></thead><tbody>%(rows)s</tbody></table>
<div class="controls" role="toolbar">
  <button data-o="classyfire" aria-pressed="true">ClassyFire</button>
  <button data-o="npclassifier" aria-pressed="false">NPClassifier</button>
  <span class="sep"></span>
  <button data-u="1" aria-pressed="true">With unknown</button>
  <button data-u="0" aria-pressed="false">Without unknown</button>
</div>
<div id="chart"></div>
<p class="note">Click a sector to zoom in, click the centre to zoom out. Two levels are shown at a time. &laquo;&nbsp;Unknown&nbsp;&raquo;: no class in the database (empty or NOT FOUND). Multiple NPClassifier classifications (&laquo;&nbsp;Alkaloids | Terpenoids&nbsp;&raquo;) are kept as written in the database. Without unknown, molecules with no class at the first level are left out, and a sector whose children are partly unclassified keeps its full size.</p>
</main>
<script>
const CHARTS = %(data)s;
let ontology = 'classyfire', unknown = '1';
function dark() { return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches; }
function draw() {
  const c = CHARTS[ontology + (unknown === '1' ? '' : '_known')];
  const fg = dark() ? '#e2e8f0' : '#1e293b';
  Plotly.react('chart', [{
    type: 'sunburst', ids: c.ids, labels: c.labels, parents: c.parents, values: c.values,
    branchvalues: 'total', maxdepth: 2, insidetextorientation: 'radial', sort: false,
    marker: { colors: c.colors, line: { color: dark() ? '#0f172a' : '#ffffff', width: 1 } },
    hovertemplate: '<b>%%{label}</b><br>%%{value:,} molecules<br>%%{percentRoot:.1%%} of the chart<br>%%{percentParent:.1%%} of %%{parent}<extra></extra>'
  }], { margin: { t: 10, l: 0, r: 0, b: 10 }, paper_bgcolor: 'rgba(0,0,0,0)', font: { color: fg, size: 12 } },
  { displaylogo: false, responsive: true });
}
document.querySelectorAll('.controls button').forEach((b) => b.addEventListener('click', () => {
  if (b.dataset.o) { ontology = b.dataset.o; document.querySelectorAll('[data-o]').forEach((x) => x.setAttribute('aria-pressed', x === b)); }
  if (b.dataset.u) { unknown = b.dataset.u; document.querySelectorAll('[data-u]').forEach((x) => x.setAttribute('aria-pressed', x === b)); }
  draw();
}));
if (window.matchMedia) window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', draw);
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
                                    marker=dict(colors=c["colors"], line=dict(color="#ffffff", width=1))))
        fig.update_layout(title=dict(text="%s · FragHub internal database %s" % (title, meta["version"]), x=0.5,
                                     font=dict(size=18)),
                          margin=dict(t=60, l=10, r=10, b=10), paper_bgcolor="#ffffff",
                          font=dict(family="Helvetica, Arial, sans-serif", size=12, color="#1e293b"))
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
