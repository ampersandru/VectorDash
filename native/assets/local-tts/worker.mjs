// Optional offline speech worker. No HTTP listener and no remote text requests.
import fs from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL,fileURLToPath} from 'node:url';
import readline from 'node:readline';
const root=path.resolve(process.argv[2]);
const inRoot=(p)=>{const x=path.resolve(p);if(x!==root&&!x.startsWith(root+path.sep))throw Error('Outside local TTS storage');return x;};
globalThis.fetch=async (u)=>{const url=new URL(u);if(url.protocol!=='file:')throw Error('Local TTS network access disabled');return new Response(await fs.readFile(inRoot(fileURLToPath(url))));};
const log=console.log;console.log=(...args)=>console.error(...args);
let engine=null,model='',session=null,frontend=null;
async function load(req){
 if(model===req.model&&engine)return;
 if(session){await session.release();session=null;}engine=null;
 const base=pathToFileURL(path.join(root,'paradee','paradee-web-v2')+path.sep).href;
 const mod=await import(new URL('paradee.mjs',base));
 // The supplied bundle is an optimized CPU SIMD engine, not a GPU graph.
 if(req.model.startsWith('paradee-')){
  if(req.device!=='CPU')throw Error('Paradee PDN uses CPU SIMD. Select CPU or use Kokoro for GPU.');
  engine=await mod.loadParadee({base,model:req.model.slice(8),lexicon:'full'});
 }else{
  const ort=await import('onnxruntime-node');
  const providers=req.device==='GPU (DirectML)'?['dml','cpu']:['cpu'];
  if(req.device!=='CPU'&&req.device!=='GPU (DirectML)')throw Error('Unsupported speech device');
  session=await ort.InferenceSession.create(path.join(root,'kokoro',req.model==='kokoro-fp16'?'model_fp16.onnx':'model_quantized.onnx'),{executionProviders:providers,enableMemPattern:false,executionMode:'sequential',intraOpNumThreads:2});
  const lex=await import(new URL('frontend/misaki_lite.mjs',base));
  const esp=await import(new URL('frontend/espeak_chain.mjs',base));
  const tok=await import(new URL('frontend/tokenizer.mjs',base));
  esp.setEspeak(()=>import(new URL('frontend/kokoro-phonemize.mjs',base)));
  lex.useFullLexicon(lex.parseLexicon(await fs.readFile(new URL('frontend/us_gold.fc.txt',base),'utf8')),lex.parseLexicon(await fs.readFile(new URL('frontend/us_silver.fc.txt',base),'utf8')));
  await tok.loadTokenizer(JSON.parse(await fs.readFile(new URL('frontend/tokenizer.json',base),'utf8')));
  engine={async *stream(text){
   for(const sentence of mod.splitSentences(text)){
    const ps=await lex.g2p(sentence);let chars=[...ps];
    while(chars.length){let n=Math.min(480,chars.length);if(n<chars.length){const split=chars.slice(0,n).lastIndexOf(' ');if(split>100)n=split;}const ids=tok.tokenize(chars.splice(0,n).join(''));
     const styleBytes=await fs.readFile(path.join(root,'kokoro',req.voice+'.bin'));const styles=new Float32Array(styleBytes.buffer,styleBytes.byteOffset,styleBytes.byteLength/4);
     const start=(ids.length-2)*256;if(start+256>styles.length)throw Error('Voice style index out of bounds');
     const feeds={input_ids:new ort.Tensor('int64',BigInt64Array.from(ids,BigInt),[1,ids.length]),style:new ort.Tensor('float32',styles.slice(start,start+256),[1,256]),speed:new ort.Tensor('float32',new Float32Array([1]),[1])};
     const result=await session.run(feeds);const value=result[session.outputNames[0]];yield Float32Array.from(value.data);for(const v of Object.values(result))v.dispose?.();
    }
   }
  }};
 }
 model=req.model;
}
function wav(samples){const out=Buffer.alloc(44+samples.length*2);out.write('RIFF');out.writeUInt32LE(out.length-8,4);out.write('WAVEfmt ',8);out.writeUInt32LE(16,16);out.writeUInt16LE(1,20);out.writeUInt16LE(1,22);out.writeUInt32LE(24000,24);out.writeUInt32LE(48000,28);out.writeUInt16LE(2,32);out.writeUInt16LE(16,34);out.write('data',36);out.writeUInt32LE(samples.length*2,40);for(let i=0;i<samples.length;i++)out.writeInt16LE(Math.round(Math.max(-1,Math.min(1,samples[i]))*32767),44+i*2);return out;}
for await(const line of readline.createInterface({input:process.stdin,crlfDelay:Infinity})){
 try{const req=JSON.parse(line);if(!['paradee-v2f_lite','paradee-v2f','paradee-v2g','kokoro-fp16','kokoro-q8'].includes(req.model))throw Error('Unknown speech model');if(!['af_heart','af_bella','am_adam','am_michael'].includes(req.voice))throw Error('Unknown voice');await load(req);if(req.op==='prepare'){log(JSON.stringify({ready:true}));continue;}if(req.op!=='synth'||typeof req.text!=='string'||req.text.length>12000)throw Error('Invalid speech request');let n=0,parts=[];for await(const y of engine.stream(req.text)){n+=y.length;if(n>24000*600)throw Error('Speech audio limit exceeded');parts.push(y);}const all=new Float32Array(n);let o=0;for(const p of parts){all.set(p,o);o+=p.length;}log(JSON.stringify({audio:wav(all).toString('base64'),samples:n,device:req.device}));}
 catch(e){log(JSON.stringify({error:String(e.message||e).slice(0,400)}));}
}
