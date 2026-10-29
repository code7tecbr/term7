#!/usr/bin/env python3
"""Gera a fonte do modo anônimo: uma mono em que as letras e os números
desenham outros caracteres. A tela parece texto normal, mas não dá pra ler.

Várias letras usam o mesmo desenho (vogal vira uma de 3 vogais, consoante uma
de 7 consoantes, dígito um de 3 dígitos). Assim a informação some de verdade:
uma troca 1:1 dava pra decifrar numa gravação pela frequência das letras.
Pontuação e símbolos ficam como estão, pra manter o formato (caminhos, URLs).

Uso (precisa do fontTools e do brotli):
    python3 scripts/anon-font.py /usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf src/assets/term7-anon.woff2
A DejaVu Sans Mono (licença Bitstream Vera) permite derivar desde que mude o nome.
"""

import sys
import unicodedata

from fontTools import subset
from fontTools.ttLib import TTFont

NAME = "term7 Anon"

LOWER_VOWELS, LOWER_CONS = "eoa", "nrstlmd"
UPPER_VOWELS, UPPER_CONS = "EAO", "RSTNMLD"
DIGITS = "472"

# Latim (com acentos), pontuação, setas, desenho de caixa: o que o terminal usa.
KEEP = "U+0020-024F,U+2000-206F,U+20AC,U+2190-21FF,U+2500-259F"


def pick(ch: str, pool: str) -> str:
    # Fixo por letra, pra mesma palavra sempre sair igual (parece texto de verdade).
    return pool[(ord(ch) * 31 + 11) % len(pool)]


def target(cp: int) -> str | None:
    ch = chr(cp)
    base = unicodedata.normalize("NFD", ch)[0]
    if base.isascii() and base.isalpha():
        vowel = base.lower() in "aeiou"
        if base.islower():
            return pick(base, LOWER_VOWELS if vowel else LOWER_CONS)
        return pick(base, UPPER_VOWELS if vowel else UPPER_CONS)
    if ch.isascii() and ch.isdigit():
        return pick(ch, DIGITS)
    return None


def main(src: str, out: str) -> None:
    font = TTFont(src)
    opts = subset.Options()
    opts.layout_features = []  # sem ligaduras: elas juntariam letras reais
    opts.name_IDs = ["*"]
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=subset.parse_unicodes(KEEP))
    sub.subset(font)

    for table in font["cmap"].tables:
        if not table.isUnicode():
            continue
        original = dict(table.cmap)
        for cp in original:
            t = target(cp)
            if t and ord(t) in original:
                table.cmap[cp] = original[ord(t)]

    for rec in font["name"].names:
        if rec.nameID in (1, 3, 4, 6, 16):
            rec.string = NAME if rec.nameID != 6 else NAME.replace(" ", "")
    font.flavor = "woff2"
    font.save(out)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
