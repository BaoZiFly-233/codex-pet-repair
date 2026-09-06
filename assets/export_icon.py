"""Export Windows ICO from the approved square SVG; no source PNG is used."""
from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image

root = Path(__file__).resolve().parent
svg = ET.parse(root / 'pet-repair-pixel.svg').getroot()
image = Image.new('RGBA', (56, 56))
for node in svg:
    if node.tag.rsplit('}', 1)[-1] != 'rect':
        continue
    x, y, w, h = (int(node.get(k)) for k in ('x', 'y', 'width', 'height'))
    color = tuple(bytes.fromhex(node.get('fill')[1:])) + (255,)
    for row in range(y, y+h):
        for col in range(x, x+w):
            image.putpixel((col, row), color)
image.resize((256, 256), Image.Resampling.NEAREST).save(root / 'pet-repair.ico', sizes=[(16,16),(20,20),(24,24),(32,32),(40,40),(48,48),(64,64),(128,128),(256,256)])
print(root / 'pet-repair.ico')
