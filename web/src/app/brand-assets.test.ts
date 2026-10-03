import {readFileSync} from "node:fs";
import {resolve} from "node:path";
import {describe,it,expect} from "vitest";
const root=resolve(process.cwd());const publicDir=resolve(root,"public");
const read=(name:string)=>readFileSync(resolve(publicDir,name));
describe("browser brand assets",()=>{
 it("publishes the control-center identity and all icon links without read-only framing",()=>{
  const html=readFileSync(resolve(root,"index.html"),"utf8");
  expect(html).toContain("be6500panel · 控制中心");expect(html).not.toMatch(/只读|read.only/i);
  for(const name of ["favicon.svg","favicon.ico","favicon-16x16.png","favicon-32x32.png","apple-touch-icon.png","site.webmanifest"]) expect(html).toContain(`/${name}`);
  const svg=read("favicon.svg").toString("utf8");expect(svg).toContain('viewBox="0 0 64 64"');expect(svg).not.toMatch(/<script|href=|foreignObject/);
 });
 it("contains exact PNG dimensions and three valid PNG-compressed ICO entries",()=>{
  for(const [name,size] of [["favicon-16x16.png",16],["favicon-32x32.png",32],["favicon-48x48.png",48],["apple-touch-icon.png",180],["icon-192.png",192],["icon-512.png",512]] as const){
   const data=read(name);expect(data.subarray(0,8).toString("hex")).toBe("89504e470d0a1a0a");expect(data.readUInt32BE(16)).toBe(size);expect(data.readUInt32BE(20)).toBe(size);
  }
  const ico=read("favicon.ico");expect(ico.readUInt16LE(0)).toBe(0);expect(ico.readUInt16LE(2)).toBe(1);expect(ico.readUInt16LE(4)).toBe(3);
  [16,32,48].forEach((size,i)=>{const pos=6+16*i;expect(ico[pos]).toBe(size);expect(ico[pos+1]).toBe(size);const bytes=ico.readUInt32LE(pos+8),offset=ico.readUInt32LE(pos+12);expect(offset+bytes).toBeLessThanOrEqual(ico.length);expect(ico.subarray(offset,offset+8).toString("hex")).toBe("89504e470d0a1a0a");});
 });
 it("maps mobile home-screen icons without registering offline behavior",()=>{
  const manifest=JSON.parse(read("site.webmanifest").toString("utf8"));
  expect(manifest.name).toBe("be6500panel · 控制中心");expect(manifest.start_url).toBe("/");expect(manifest.icons.map((icon:any)=>icon.sizes)).toEqual(["192x192","512x512"]);
  expect(manifest.icons.every((icon:any)=>read(icon.src.slice(1)).length>0)).toBe(true);expect(manifest).not.toHaveProperty("service_worker");
 });
});
