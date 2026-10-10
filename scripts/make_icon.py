"""Gera o ícone provisório do app (1024px) e converte para .ico multi-tamanho.

Substituir por arte final depois: basta trocar src-tauri/icons/source.png e
rodar `npx tauri icon src-tauri/icons/source.png`.
"""
from PIL import Image, ImageDraw
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

size = 1024
img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)
# Fundo arredondado azul.
draw.rounded_rectangle((64, 64, size - 64, size - 64), radius=220, fill=(37, 99, 235, 255))
# Ondas de áudio em branco (representam gravação).
bars = [260, 380, 520, 640, 520, 380, 260]
x0, gap, w = 232, 70, 60
for i, h in enumerate(bars):
    x = x0 + i * gap
    draw.rounded_rectangle((x, size // 2 - h // 2, x + w, size // 2 + h // 2), radius=30, fill=(255, 255, 255, 255))

img.save(OUT / "source.png")
img.save(OUT / "icon.ico", sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)])
img.resize((512, 512)).save(OUT / "icon.png")
print("ícone gerado em", OUT)
