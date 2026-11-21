"""Transcript: the line range a partial file read shows beside its path."""

_KEYS = ["lines %{start}–%{end}", "lines %{start}–%{end} of %{total}"]

_T = {
    "zh-CN": ["第 %{start}–%{end} 行", "第 %{start}–%{end} 行，共 %{total} 行"],
    "ja": ["%{start}–%{end} 行目", "%{start}–%{end} 行目（全 %{total} 行）"],
    "ko": ["%{start}–%{end}행", "%{start}–%{end}행 / 전체 %{total}행"],
    "es": ["líneas %{start}–%{end}", "líneas %{start}–%{end} de %{total}"],
    "fr": ["lignes %{start}–%{end}", "lignes %{start}–%{end} sur %{total}"],
    "de": ["Zeilen %{start}–%{end}", "Zeilen %{start}–%{end} von %{total}"],
    "pt-BR": ["linhas %{start}–%{end}", "linhas %{start}–%{end} de %{total}"],
    "ru": ["строки %{start}–%{end}", "строки %{start}–%{end} из %{total}"],
    "it": ["righe %{start}–%{end}", "righe %{start}–%{end} di %{total}"],
}

GLOSSARY = {locale: dict(zip(_KEYS, values)) for locale, values in _T.items()}
assert all(len(values) == len(_KEYS) for values in _T.values())
