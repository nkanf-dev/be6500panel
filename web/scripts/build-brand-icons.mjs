import { chromium } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const publicDir = new URL("../public/", import.meta.url);
const svg = await readFile(new URL("favicon.svg", publicDir), "utf8");
const sizes = new Map([[16,"favicon-16x16.png"],[32,"favicon-32x32.png"],[48,"favicon-48x48.png"],[180,"apple-touch-icon.png"],[192,"icon-192.png"],[512,"icon-512.png"]]);
const browser = await chromium.launch({executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined});
try {
 const page = await browser.newPage({deviceScaleFactor:1});
 for (const [size,name] of sizes) {
  await page.setViewportSize({width:size,height:size});
  const sized = svg.replace(/width="64" height="64"/, `width="${size}" height="${size}"`);
  await page.setContent(`<style>html,body{margin:0;padding:0;overflow:hidden;background:transparent;width:${size}px;height:${size}px}svg{display:block;overflow:hidden;width:${size}px;height:${size}px}</style>${sized}`);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await page.screenshot({path:fileURLToPath(new URL(name,publicDir)),omitBackground:true,clip:{x:0,y:0,width:size,height:size}});
 }
 // ICO permits PNG-compressed entries. Embed all three exact resolutions.
 const buffers = await Promise.all([16,32,48].map(size=>readFile(new URL(sizes.get(size),publicDir))));
 const header = Buffer.alloc(6+16*buffers.length);header.writeUInt16LE(1,2);header.writeUInt16LE(buffers.length,4);
 let offset=header.length;
 buffers.forEach((png,i)=>{const start=6+16*i,size=[16,32,48][i];header[start]=size;header[start+1]=size;header.writeUInt16LE(1,start+4);header.writeUInt16LE(32,start+6);header.writeUInt32LE(png.length,start+8);header.writeUInt32LE(offset,start+12);offset+=png.length;});
 await writeFile(new URL("favicon.ico",publicDir),Buffer.concat([header,...buffers]));
 console.log("Rendered six exact-size PNGs and a three-resolution ICO from favicon.svg.");
} finally {await browser.close();}
