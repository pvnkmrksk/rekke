#!/usr/bin/env python3
"""Minimal live Kinefly ZMQ plot. Dependency: python3 -m pip install pyzmq."""
import argparse
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import threading
import time
import webbrowser

PAGE = r'''<!doctype html><meta charset="utf-8"><title>Kinefly live</title>
<style>
html,body{height:100%}body{display:flex;flex-direction:column;overflow:hidden;margin:0;background:#15181d;color:#e9edf1;font:15px system-ui}
header{flex-shrink:0;display:flex;gap:22px;align-items:center;padding:16px 22px;flex-wrap:wrap}
header b{font-size:19px} #status{color:#9aa5b1;font-size:13px}
canvas{display:block;width:100%;height:100%;cursor:crosshair;touch-action:none}
button{background:#2a303a;color:#e9edf1;border:1px solid #454e5d;border-radius:5px;padding:6px 12px;cursor:pointer}
#chart{position:relative;flex:1;min-height:0}#tip{position:absolute;display:none;pointer-events:none;background:#242a34;border:1px solid #586373;border-radius:4px;padding:7px;font:12px monospace;white-space:nowrap}
small{flex-shrink:0;display:block;font-size:12px;margin-bottom:8px;color:#9aa5b1;padding:0 22px}
</style>
<header><b>Kinefly live</b><span style="color:#ff7078">Left <strong id="l">—</strong></span>
<span style="color:#69da87">Right <strong id="r">—</strong></span>
<span style="color:#78b7ff">L−R <strong id="d">—</strong></span><button id="pause">Pause</button><button id="reset">Reset view</button><span id="status">Connecting…</span></header>
<small>Radians · <span id="window">10.000 s window</span> · scroll: zoom time · Shift+scroll: zoom angles · drag: pan · double-click: reset · Space: pause/resume</small><div id="chart"><canvas id="plot"></canvas><div id="tip"></div></div>
<script>
const canvas=document.getElementById('plot'),ctx=canvas.getContext('2d');
const pauseButton=document.getElementById('pause'),tip=document.getElementById('tip');
let data={points:[],now:0,status:'Waiting for ZMQ…'},latest=data,paused=false,view=null,hover=null,drag=null;
function bounds(){return {left:64,right:canvas.clientWidth-22,top:18,bottom:canvas.clientHeight-42};}
function defaults(){
 let range=.1;for(const p of data.points)for(let i=1;i<=3;i++)range=Math.max(range,Math.abs(p[i]));range*=1.1;
 return {t0:data.now-10,t1:data.now,y0:-range,y1:range};
}
function clampTime(v){const span=v.t1-v.t0;v.t0=Math.max(data.now-10,Math.min(data.now-span,v.t0));v.t1=v.t0+span;}
function setPaused(value){paused=value;view=paused?(view||defaults()):null;hover=null;pauseButton.textContent=paused?'Resume live':'Pause';draw();}
function reset(){view=paused?defaults():null;hover=null;draw();}
function nearest(time){
 const p=data.points;if(!p.length)return null;let lo=0,hi=p.length-1;
 while(lo<hi){const mid=(lo+hi)>>1;if(p[mid][0]<time)lo=mid+1;else hi=mid;}
 return lo>0&&Math.abs(p[lo-1][0]-time)<Math.abs(p[lo][0]-time)?p[lo-1]:p[lo];
}
function draw(){
 const w=canvas.clientWidth,h=canvas.clientHeight,ratio=devicePixelRatio||1;
 canvas.width=Math.round(w*ratio);canvas.height=Math.round(h*ratio);ctx.scale(ratio,ratio);
 const {left,right,top,bottom}=bounds(),v=view||defaults();
 const x=t=>left+(t-v.t0)/(v.t1-v.t0)*(right-left),y=n=>bottom-(n-v.y0)/(v.y1-v.y0)*(bottom-top);
 ctx.font='12px system-ui';ctx.lineWidth=1;
 for(let n=0;n<=4;n++){
  const value=v.y0+(v.y1-v.y0)*n/4,py=y(value);ctx.strokeStyle='#303640';
  ctx.beginPath();ctx.moveTo(left,py);ctx.lineTo(right,py);ctx.stroke();
  ctx.fillStyle='#9aa5b1';ctx.fillText(value.toFixed(3),5,py+4);
 }
 if(v.y0<0&&v.y1>0){ctx.strokeStyle='#515d6b';ctx.beginPath();ctx.moveTo(left,y(0));ctx.lineTo(right,y(0));ctx.stroke();}
 for(let n=0;n<=5;n++){
  const t=v.t0+(v.t1-v.t0)*n/5,px=x(t);ctx.fillStyle='#9aa5b1';
  ctx.fillText((t-data.now).toFixed(v.t1-v.t0<1?3:1)+' s',px-18,bottom+24);
 }
 ctx.save();ctx.beginPath();ctx.rect(left,top,right-left,bottom-top);ctx.clip();
 const colors=['#ff7078','#69da87','#78b7ff'];
 colors.forEach((color,index)=>{
  ctx.strokeStyle=color;ctx.lineWidth=1.6;ctx.beginPath();let previous;
  for(const p of data.points){
   if(p[0]<v.t0||p[0]>v.t1)continue;
   if(previous===undefined||p[0]-previous>.5)ctx.moveTo(x(p[0]),y(p[index+1]));
   else ctx.lineTo(x(p[0]),y(p[index+1]));previous=p[0];
  }ctx.stroke();
 });
 let selected=data.points[data.points.length-1];
 if(hover&&!drag){
  const time=v.t0+(hover.x-left)/(right-left)*(v.t1-v.t0);selected=nearest(time);
  if(selected){
   ctx.setLineDash([3,3]);ctx.strokeStyle='#9aa5b1';ctx.lineWidth=1;
   ctx.beginPath();ctx.moveTo(x(selected[0]),top);ctx.lineTo(x(selected[0]),bottom);ctx.stroke();ctx.setLineDash([]);
   colors.forEach((color,i)=>{ctx.fillStyle=color;ctx.beginPath();ctx.arc(x(selected[0]),y(selected[i+1]),3,0,Math.PI*2);ctx.fill();});
   tip.textContent=`${(selected[0]-data.now).toFixed(3)} s  ·  L ${selected[1].toFixed(4)}  R ${selected[2].toFixed(4)}  L−R ${selected[3].toFixed(4)} rad`;
   tip.style.display='block';tip.style.left=Math.max(0,Math.min(hover.x+12,w-360))+'px';tip.style.top=Math.max(top,hover.y-32)+'px';
  }
 }else tip.style.display='none';
 ctx.restore();
 if(selected)['l','r','d'].forEach((id,i)=>document.getElementById(id).textContent=selected[i+1].toFixed(4));
 document.getElementById('status').textContent=(paused?'Paused · ':'')+latest.status;
 document.getElementById('window').textContent=(v.t1-v.t0).toFixed(3)+' s window';
}
function pointer(event){const rect=canvas.getBoundingClientRect();return {x:event.clientX-rect.left,y:event.clientY-rect.top};}
canvas.addEventListener('wheel',event=>{
 event.preventDefault();setPaused(true);
 const p=pointer(event),b=bounds(),v=view,factor=Math.exp(Math.max(-200,Math.min(200,event.deltaY))*.003);
 if(!event.shiftKey||event.ctrlKey){
  const f=Math.max(0,Math.min(1,(p.x-b.left)/(b.right-b.left))),anchor=v.t0+f*(v.t1-v.t0);
  const span=Math.max(.002,Math.min(10,(v.t1-v.t0)*factor));v.t0=anchor-f*span;v.t1=v.t0+span;clampTime(v);
 }
 if(event.shiftKey||event.ctrlKey){
  const f=Math.max(0,Math.min(1,(b.bottom-p.y)/(b.bottom-b.top))),anchor=v.y0+f*(v.y1-v.y0);
  const span=Math.max(.00001,Math.min(100,(v.y1-v.y0)*factor));v.y0=anchor-f*span;v.y1=v.y0+span;
 }
 hover=p;draw();
},{passive:false});
canvas.addEventListener('pointerdown',event=>{
 if(event.button!==0)return;setPaused(true);const p=pointer(event);drag={...p,view:{...view}};
 canvas.setPointerCapture(event.pointerId);canvas.style.cursor='grabbing';tip.style.display='none';
});
canvas.addEventListener('pointermove',event=>{
 const p=pointer(event);hover=p;
 if(drag){
  const b=bounds(),old=drag.view,dt=-(p.x-drag.x)/(b.right-b.left)*(old.t1-old.t0),dy=(p.y-drag.y)/(b.bottom-b.top)*(old.y1-old.y0);
  view={t0:old.t0+dt,t1:old.t1+dt,y0:old.y0+dy,y1:old.y1+dy};clampTime(view);
 }draw();
});
canvas.addEventListener('pointerup',()=>{drag=null;canvas.style.cursor='crosshair';draw();});
canvas.addEventListener('pointercancel',()=>{drag=null;canvas.style.cursor='crosshair';});
canvas.addEventListener('pointerleave',()=>{if(!drag){hover=null;tip.style.display='none';draw();}});
canvas.addEventListener('dblclick',reset);
pauseButton.addEventListener('click',()=>setPaused(!paused));
document.getElementById('reset').addEventListener('click',reset);
addEventListener('keydown',event=>{if(event.code==='Space'&&!['INPUT','BUTTON'].includes(document.activeElement.tagName)){event.preventDefault();setPaused(!paused);}});
async function poll(){
 try{const response=await fetch('/data',{cache:'no-store'});latest=await response.json();if(!paused)data=latest;draw();}
 catch(e){document.getElementById('status').textContent='Plot server disconnected';}
 setTimeout(poll,50);
}
addEventListener('resize',draw);poll();
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default='127.0.0.1', help='Kinefly publisher computer')
    parser.add_argument('--port', type=int, default=9871, help='ZMQ publisher port')
    parser.add_argument('--http-port', type=int, default=8765, help='Local plot browser port')
    parser.add_argument('--no-browser', action='store_true')
    args = parser.parse_args()
    try:
        import zmq
    except ImportError:
        parser.exit(1, 'Install the one dependency first: python3 -m pip install pyzmq\n')
    endpoint = f'tcp://{args.host}:{args.port}'
    points = deque(maxlen=6000)
    lock = threading.Lock()
    stop = threading.Event()
    latest = {'time': None, 'error': None}

    def receive():
        context = zmq.Context()
        socket = context.socket(zmq.SUB)
        socket.setsockopt(zmq.SUBSCRIBE, b'')
        socket.setsockopt(zmq.RCVHWM, 2000)
        socket.setsockopt(zmq.RCVTIMEO, 100)
        socket.connect(endpoint)
        try:
            while not stop.is_set():
                try:
                    value = socket.recv_json()
                    left, right = float(value['x']), float(value['y'])
                    difference = float(value.get('yaw', left-right))
                    if not all(map(math.isfinite, (left, right, difference))):
                        continue
                    now = time.monotonic()
                    with lock:
                        points.append((now, left, right, difference))
                        while points and points[0][0] < now-10:
                            points.popleft()
                        latest.update(time=now, error=None)
                except zmq.Again:
                    pass
                except (ValueError, KeyError, TypeError):
                    with lock:
                        latest['error'] = 'Expected Kinefly JSON fields x, y, yaw'
        finally:
            socket.close(linger=0)
            context.term()

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == '/data':
                now = time.monotonic()
                with lock:
                    live = latest['time'] is not None and now-latest['time'] < 2
                    body = json.dumps({'now': now, 'points': [p for p in points if p[0] >= now-10],
                                       'status': latest['error'] or f'{"Live" if live else "Waiting"} · {endpoint}'}).encode()
                content_type = 'application/json'
            elif self.path == '/':
                body, content_type = PAGE.encode(), 'text/html; charset=utf-8'
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header('Content-Type', content_type)
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', args.http_port), Handler)
    worker = threading.Thread(target=receive, daemon=True)
    worker.start()
    url = f'http://127.0.0.1:{server.server_port}'
    print(f'Plot: {url}  |  ZMQ: {endpoint}  |  Ctrl+C to stop', flush=True)
    if not args.no_browser:
        webbrowser.open(url)
    try:
        server.serve_forever(poll_interval=.1)
    except KeyboardInterrupt:
        pass
    finally:
        stop.set()
        server.server_close()
        worker.join(timeout=2)


if __name__ == '__main__':
    main()
