//! Dedicated Chromium profile and a bounded, typed CDP client. Never attaches to daily browser tabs.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};
pub fn validate_url(url: &str) -> Result<()> {
    ensure!(
        url.len() <= 4096 && !url.chars().any(char::is_control),
        "Invalid URL"
    );
    let parsed = reqwest::Url::parse(url).context("Use an HTTP(S) URL")?;
    ensure!(
        matches!(parsed.scheme(), "http" | "https")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none(),
        "Only HTTP(S) URLs without embedded credentials are supported"
    );
    Ok(())
}
pub fn browser_path(configured: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        let p = PathBuf::from(configured);
        return p.is_file().then_some(p);
    }
    for env in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(env) {
            for relative in [
                "Microsoft/Edge/Application/msedge.exe",
                "Google/Chrome/Application/chrome.exe",
            ] {
                let p = PathBuf::from(&root).join(relative);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}
pub struct Browser {
    child: Child,
    socket: WebSocket<TcpStream>,
    serial: u64,
    session: String,
    context: Option<i64>,
    snapshot: Option<String>,
    page: (usize, usize),
    key: String,
    _profile: PathBuf,
}
impl Browser {
    pub fn start(configured: &str, visible: bool, base: &std::path::Path) -> Result<Self> {
        let executable = browser_path(configured).context(
            "Install Microsoft Edge or Google Chrome, or select its executable in AI settings",
        )?;
        let profile = base.join(format!("browser-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&profile)?;
        let mut command = Command::new(executable);
        command
            .arg(format!("--user-data-dir={}", profile.display()))
            .args([
                "--remote-debugging-port=0",
                "--remote-debugging-address=127.0.0.1",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-networking",
                "--disable-sync",
                "--disable-extensions",
                "--disable-component-update",
                "--disable-default-apps",
                "--window-size=1100,800",
                "about:blank",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !visible {
            command.arg("--headless=new");
        }
        crate::content::hidden(&mut command);
        let mut child = command
            .spawn()
            .context("Could not start the dedicated browser")?;
        let result = (|| -> Result<_> {
            let start = Instant::now();
            let (port, route) = loop {
                if let Ok(text) = std::fs::read_to_string(profile.join("DevToolsActivePort")) {
                    let mut lines = text.lines();
                    if let (Some(port), Some(route)) = (
                        lines.next().and_then(|v| v.parse::<u16>().ok()),
                        lines.next(),
                    ) {
                        ensure!(
                            route.starts_with("/devtools/browser/")
                                && route.len() < 200
                                && !route.contains(['\r', '\n']),
                            "Invalid browser endpoint"
                        );
                        break (port, route.to_owned());
                    }
                }
                ensure!(
                    child.try_wait()?.is_none(),
                    "Browser exited before starting its control endpoint"
                );
                ensure!(
                    start.elapsed() < Duration::from_secs(12),
                    "Browser startup timed out"
                );
                std::thread::sleep(Duration::from_millis(60));
            };
            let address = SocketAddr::from(([127, 0, 0, 1], port));
            let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
            stream.set_read_timeout(Some(Duration::from_secs(6)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            let config = WebSocketConfig::default()
                .max_message_size(Some(4 * 1024 * 1024))
                .max_frame_size(Some(4 * 1024 * 1024));
            let (socket, _) = tungstenite::client::client_with_config(
                format!("ws://127.0.0.1:{port}{route}"),
                stream,
                Some(config),
            )
            .map_err(|e| anyhow::anyhow!("Browser WebSocket handshake failed: {e}"))?;
            Ok(socket)
        })();
        let socket = match result {
            Ok(v) => v,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let mut browser = Self {
            child,
            socket,
            serial: 0,
            session: String::new(),
            context: None,
            snapshot: None,
            page: (0, 0),
            key: format!("vd{:016x}", rand::random::<u64>()),
            _profile: profile,
        };
        let target = browser.call("Target.createTarget", json!({"url":"about:blank"}), false)?;
        let session = browser.call(
            "Target.attachToTarget",
            json!({"targetId":target["targetId"],"flatten":true}),
            false,
        )?;
        browser.session = session["sessionId"]
            .as_str()
            .context("Browser did not create a page session")?
            .into();
        browser.call(
            "Browser.setDownloadBehavior",
            json!({"behavior":"deny"}),
            false,
        )?;
        // One owned page is enough; avoid carrying an unused startup tab.
        if let Ok(targets) = browser.call("Target.getTargets", json!({}), false) {
            for item in targets["targetInfos"].as_array().into_iter().flatten() {
                if item["type"] == "page" && item["targetId"] != target["targetId"] {
                    let _ = browser.call(
                        "Target.closeTarget",
                        json!({"targetId":item["targetId"]}),
                        false,
                    );
                }
            }
        }
        Ok(browser)
    }
    fn call(&mut self, method: &str, parameters: Value, page: bool) -> Result<Value> {
        self.serial += 1;
        let id = self.serial;
        let mut request = json!({"id":id,"method":method,"params":parameters});
        if page {
            request["sessionId"] = json!(self.session);
        }
        self.socket
            .send(Message::Text(request.to_string().into()))?;
        let start = Instant::now();
        for _ in 0..512 {
            ensure!(
                start.elapsed() < Duration::from_secs(8),
                "Browser command timed out"
            );
            let message = self
                .socket
                .read()
                .with_context(|| format!("Browser command {method} failed"))?;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["id"].as_u64() == Some(id) {
                        ensure!(
                            value.get("error").is_none(),
                            "Browser command failed: {}",
                            value["error"]
                        );
                        return Ok(value["result"].clone());
                    }
                }
                Message::Close(_) => anyhow::bail!("Browser closed"),
                _ => (),
            }
        }
        anyhow::bail!("Browser event limit reached")
    }
    fn world(&mut self) -> Result<i64> {
        if let Some(id) = self.context {
            return Ok(id);
        }
        let tree = self.call("Page.getFrameTree", json!({}), true)?;
        let world=self.call("Page.createIsolatedWorld",json!({"frameId":tree["frameTree"]["frame"]["id"],"worldName":self.key,"grantUniveralAccess":false}),true)?;
        let id = world["executionContextId"]
            .as_i64()
            .context("Browser page is not ready")?;
        self.context = Some(id);
        Ok(id)
    }
    fn eval(&mut self, expression: String) -> Result<Value> {
        let id = self.world()?;
        let result=self.call("Runtime.evaluate",json!({"expression":expression,"contextId":id,"returnByValue":true,"awaitPromise":false,"timeout":3000}),true);
        let result = match result {
            Ok(value) => value,
            Err(error) => {
                self.context = None;
                self.snapshot = None;
                self.page = (0, 0);
                return Err(error.context(
                    "Page changed or browser connection failed; request a fresh snapshot",
                ));
            }
        };
        if result.get("exceptionDetails").is_some() {
            self.context = None;
            self.snapshot = None;
            self.page = (0, 0);
            anyhow::bail!("Page changed or action failed; take a fresh snapshot");
        }
        let value = result["result"]["value"].clone();
        ensure!(
            !value.is_null(),
            "Browser returned no result; take a fresh snapshot"
        );
        if let Some(error) = value["error"].as_str() {
            anyhow::bail!("{error}");
        }
        Ok(value)
    }
    fn ready(&mut self) -> Result<()> {
        self.page = (0, 0);
        self.context = None;
        self.snapshot = None;
        std::thread::sleep(Duration::from_millis(150));
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if self
                .eval("({ready:document.readyState})".into())
                .ok()
                .is_some_and(|v| v["ready"] != "loading")
            {
                break;
            }
            self.context = None;
            std::thread::sleep(Duration::from_millis(100));
        }
        self.context = None;
        Ok(())
    }
    fn snapshot(&mut self, offset: usize, text_offset: usize) -> Result<Value> {
        let nonce = format!("b{:016x}", rand::random::<u64>());
        let expression = format!(
            r#"(()=>{{
          const nodes=Array.from(document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],[contenteditable="true"]')).slice(0,600).filter(e=>{{let r=e.getBoundingClientRect();return r.width>0&&r.height>0&&getComputedStyle(e).visibility!=='hidden';}});
          const info=nodes.map((e,i)=>({{id:i,tag:e.tagName.toLowerCase(),role:e.getAttribute('role'),name:(e.getAttribute('aria-label')||e.innerText||e.getAttribute('placeholder')||e.getAttribute('title')||'').slice(0,180),type:e.type||'',href:e.tagName==='A'?e.href:'',disabled:!!e.disabled,password:e.type==='password',focused:e===document.activeElement,value:e.type==='password'?'':String(e.value||'').slice(0,250)}}));
          globalThis[{key}]={{id:{nonce},url:location.href,nodes,info}};
          return {{offset:{offset},next_offset:info.length>{offset}+60?{offset}+60:null,total_elements:info.length,snapshot_id:{nonce},url:location.href,title:document.title.slice(0,300),text_offset:{text_offset},next_text_offset:(document.body?.innerText||'').length>{text_offset}+6000?{text_offset}+6000:null,text:(document.body?.innerText||'').slice({text_offset},{text_offset}+6000),elements:info.slice({offset},{offset}+60)}};
        }})()"#,
            key = json!(self.key),
            nonce = json!(nonce)
        );
        let value = self.eval(expression)?;
        let url = value["url"].as_str().unwrap_or("");
        if url != "about:blank" {
            validate_url(url)?;
        }
        self.snapshot = Some(nonce);
        self.page = (offset, text_offset);
        Ok(value)
    }
    pub fn execute(&mut self, args: &Value, vision: bool) -> Result<Value> {
        let action = args["action"].as_str().unwrap_or("");
        if matches!(action, "open" | "navigate") {
            let url = args["url"].as_str().context("Provide URL")?;
            validate_url(url)?;
            let response = self.call("Page.navigate", json!({"url":url}), true)?;
            ensure!(
                response.get("errorText").is_none(),
                "Navigation failed: {}",
                response["errorText"]
            );
            self.ready()?;
            return self.snapshot(0, 0);
        }
        if action == "snapshot" {
            return self.snapshot(
                args["offset"].as_u64().unwrap_or(0).min(600) as usize,
                args["text_offset"].as_u64().unwrap_or(0).min(1000000) as usize,
            );
        }
        if action == "screenshot" {
            ensure!(
                vision,
                "Enable vision support for the selected AI model before requesting screenshots"
            );
            let mut value = self.snapshot(
                args["offset"]
                    .as_u64()
                    .unwrap_or(self.page.0 as u64)
                    .min(600) as usize,
                args["text_offset"]
                    .as_u64()
                    .unwrap_or(self.page.1 as u64)
                    .min(1_000_000) as usize,
            )?;
            let result = self.call(
                "Page.captureScreenshot",
                json!({"format":"jpeg","quality":65,"captureBeyondViewport":false}),
                true,
            )?;
            let data = result["data"].as_str().context("No browser screenshot")?;
            ensure!(
                data.len() <= 2 * 1024 * 1024,
                "Browser screenshot exceeds size limit; use the text snapshot"
            );
            value["image_url"] = json!(format!("data:image/jpeg;base64,{data}"));
            return Ok(value);
        }
        if action == "back" {
            let history = self.call("Page.getNavigationHistory", json!({}), true)?;
            let index = history["currentIndex"].as_u64().unwrap_or(0) as usize;
            ensure!(index > 0, "No previous page");
            self.call(
                "Page.navigateToHistoryEntry",
                json!({"entryId":history["entries"][index-1]["id"]}),
                true,
            )?;
            self.ready()?;
            return self.snapshot(0, 0);
        }
        let supplied = args["snapshot_id"]
            .as_str()
            .context("Take a snapshot before acting")?;
        ensure!(
            self.snapshot.as_deref() == Some(supplied),
            "Stale browser snapshot; take a fresh snapshot"
        );
        let prefix = format!(
            r#"const s=globalThis[{}];if(!s||s.id!=={}||s.url!==location.href)return {{error:'Stale snapshot; take a fresh snapshot'}};"#,
            json!(self.key),
            json!(supplied)
        );
        let result = match action {
            "click" | "type" => {
                let element = args["element"]
                    .as_u64()
                    .context("Provide an element id from snapshot")?;
                ensure!(element < 600, "Invalid element id");
                let operation = if action == "click" {
                    "if(e.tagName==='INPUT'&&e.type==='file')return {error:'File upload controls require manual input'};if(e.tagName==='A'){let u=new URL(e.href);if(!['http:','https:'].includes(u.protocol))return {error:'Only HTTP(S) links are allowed'};if(e.hasAttribute('download'))return {error:'Downloads require manual input'};e.target='_self';}e.click();".to_owned()
                } else {
                    let text = args["text"].as_str().context("Provide text")?;
                    format!(
                        r#"e.focus();if(e.isContentEditable)e.textContent={text};else{{let p=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:e.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype;let set=Object.getOwnPropertyDescriptor(p,'value')?.set;if(!set)return {{error:'Element is not an editable field'}};set.call(e,{text});}}e.dispatchEvent(new Event('input',{{bubbles:true}}));e.dispatchEvent(new Event('change',{{bubbles:true}}));"#,
                        text = json!(text)
                    )
                };
                self.eval(format!(r#"(()=>{{{prefix}const e=s.nodes[{element}];if(!e||!e.isConnected||e.disabled)return {{error:'Element changed; take a fresh snapshot'}};if(e.type==='password'||e.autocomplete==='current-password'||e.autocomplete==='new-password')return {{error:'Password fields require manual input'}};const before=s.info[{element}];const name=(e.getAttribute('aria-label')||e.innerText||e.getAttribute('placeholder')||e.getAttribute('title')||'').slice(0,180);if(!before||before.tag!==e.tagName.toLowerCase()||before.type!==(e.type||'')||before.name!==name||(e.tagName==='A'&&before.href!==e.href))return {{error:'Element changed; take a fresh snapshot'}};const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0)return {{error:'Element is hidden'}};{operation}return {{performed:true}};}})()"#))?
            }
            "scroll" => self.eval(format!(r#"(()=>{{{prefix}
                const delta={delta},id={element};let target=null;
                const visible=e=>{{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.bottom>0&&r.right>0&&r.top<innerHeight&&r.left<innerWidth&&getComputedStyle(e).visibility!=='hidden';}};
                const scrollable=e=>e&&e.scrollHeight>e.clientHeight+1;
                const movable=e=>delta>0?e.scrollTop<e.scrollHeight-e.clientHeight-1:e.scrollTop>0;
                const root=document.scrollingElement||document.documentElement;
                if(id!==null){{let e=s.nodes[id];const before=s.info[id];
                    if(!e||!e.isConnected||!before)return {{error:'Scroll target not found; take a fresh snapshot'}};
                    const name=(e.getAttribute('aria-label')||e.innerText||e.getAttribute('placeholder')||e.getAttribute('title')||'').slice(0,180);
                    if(before.tag!==e.tagName.toLowerCase()||before.name!==name)return {{error:'Scroll target changed; take a fresh snapshot'}};
                    if(e.type==='password')return {{error:'Password controls require manual input'}};
                    let boundary=null;for(let n=e;n;n=n.parentElement){{if(scrollable(n)&&visible(n)){{if(!boundary)boundary=n;if(movable(n)){{target=n;break;}}}}}}
                    target=target||boundary||root;
                }}else{{
                    let best=-1;for(const e of [root,...Array.from(document.querySelectorAll('main,article,section,[role="main"],[role="region"],div')).slice(0,1000)]){{
                        if(!visible(e)||!scrollable(e))continue;
                        if(e!==root&&!/(auto|scroll)/.test(getComputedStyle(e).overflowY))continue;
                        const r=e.getBoundingClientRect();const area=Math.max(0,Math.min(innerWidth,r.right)-Math.max(0,r.left))*Math.max(0,Math.min(innerHeight,r.bottom)-Math.max(0,r.top));
                        const rank=(movable(e)?1e15:0)+area;if(rank>best){{best=rank;target=e;}}
                    }}target=target||root;
                }}
                const before=target.scrollTop;target.scrollTo({{top:before+delta,behavior:'instant'}});const after=target.scrollTop;
                return {{performed:Math.abs(after-before)>.5,scroll_result:{{method:'dom',moved:Math.abs(after-before)>.5,at_boundary:!movable(target),before_position:before,after_position:after,delta}}}};
            }})()"#,delta=args["delta"],element=args["element"]))?,
            "key" => {
                self.eval(format!("(()=>{{{prefix}if(document.activeElement?.type==='password')return {{error:'Password fields require manual input'}};return {{ready:true}};}})()"))?;
                let codes = super::key_codes(args["keys"].as_str().unwrap_or(""))?;
                ensure!(
                    !codes
                        .iter()
                        .any(|v| [0x4f, 0x53, 0x4e, 0x57, 0x43, 0x56, 0x58].contains(v)),
                    "Browser window, file and system-clipboard shortcuts are unsupported; use typed browser actions instead"
                );
                let modifiers = if codes.contains(&0x11) { 2 } else { 0 }
                    | if codes.contains(&0x10) { 8 } else { 0 };
                for code in &codes {
                    self.call(
                        "Input.dispatchKeyEvent",
                        key_event(*code, "keyDown", modifiers),
                        true,
                    )?;
                }
                for code in codes.iter().rev() {
                    self.call(
                        "Input.dispatchKeyEvent",
                        key_event(*code, "keyUp", modifiers),
                        true,
                    )?;
                }
                json!({"performed":true})
            }
            _ => anyhow::bail!("Unsupported browser action"),
        };
        self.snapshot = None;
        let mut response = json!({"result":result,"next":"Take a fresh snapshot to verify the result before another action."});
        if let Some(report) = result.get("scroll_result") {
            response["scroll_result"] = report.clone();
            response["performed"] = result["performed"].clone();
            if report["moved"] != true {
                response["progress_warning"] = json!(
                    "No scroll movement measured. Check the boundary or choose the main content pane rather than repeating unchanged scrolling."
                );
            }
        }
        Ok(response)
    }
}
fn key_event(code: u16, kind: &str, modifiers: i32) -> Value {
    let key = match code {
        0x0d => "Enter",
        9 => "Tab",
        0x1b => "Escape",
        8 => "Backspace",
        0x20 => " ",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x11 => "Control",
        0x10 => "Shift",
        _ => "",
    };
    let key = if key.is_empty() {
        char::from_u32(code as u32)
            .unwrap_or(' ')
            .to_ascii_lowercase()
            .to_string()
    } else {
        key.into()
    };
    let mut value = json!({"type":kind,"windowsVirtualKeyCode":code,"nativeVirtualKeyCode":code,"key":key,"modifiers":modifiers});
    if kind == "keyDown" && code == 0x0d && modifiers == 0 {
        value["text"] = json!("\r");
    }
    value
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.socket.send(Message::Text(
            json!({"id":999999,"method":"Browser.close"})
                .to_string()
                .into(),
        ));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_urls_do_not_allow_local_files_or_executable_schemes() {
        for u in [
            "file:///C:/secret",
            "javascript:alert(1)",
            "data:text/html,hi",
            "chrome://settings",
            "https://user:pass@example.org",
            "https://example.org/\nfoo",
        ] {
            assert!(validate_url(u).is_err(), "{u}");
        }
        for u in ["https://example.org/?a=b", "http://127.0.0.1:8080/test"] {
            assert!(validate_url(u).is_ok());
        }
    }
}
