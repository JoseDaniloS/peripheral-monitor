import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Circle, Keyboard, ListVideo, Music, RotateCcw, Save, Square, Zap } from "lucide-react";
import { heldModifierUsages, isModifierCode, keyFromEvent, keyLabel, MEDIA_KEYS, mediaLabel, modifiersFromEvent, shortcutLabel, SPECIAL_KEYS } from "./hid";
import type { ButtonAction, MacroStep, MouseButton } from "./types";

const SLOT_NAMES: Record<number, string> = { 2: "Botão do meio", 3: "Lateral de trás", 4: "Lateral da frente" };
const MAX_MACRO_STEPS = 100;
const FIXED_DELAY_MS = 20;
const MACRO_STORE = "ajazz-macro-steps";

type ActionType = ButtonAction["type"];
const TYPES: { type: Exclude<ActionType, "unknown">; label: string }[] = [
  { type: "default", label: "Padrão" },
  { type: "keyboard", label: "Tecla / atalho" },
  { type: "media", label: "Mídia" },
  { type: "rapidFire", label: "Disparo rápido" },
  { type: "macro", label: "Macro" },
];

function loadMacro(): MacroStep[] {
  try { return JSON.parse(localStorage.getItem(MACRO_STORE) ?? "[]") as MacroStep[]; } catch { return []; }
}
function storeMacro(steps: MacroStep[]) {
  try { localStorage.setItem(MACRO_STORE, JSON.stringify(steps)); } catch { /* só para exibição */ }
}

function macroSummary(steps: MacroStep[]): string {
  const downs = steps.filter(s => s.kind === "keyDown").map(s => keyLabel(s.value));
  if (downs.length === 0) return "Macro";
  return `Macro: ${downs.slice(0, 6).join(" › ")}${downs.length > 6 ? " …" : ""}`;
}

function actionLabel(action: ButtonAction, macro: MacroStep[]): string {
  switch (action.type) {
    case "default": return "Função de fábrica";
    case "keyboard": return shortcutLabel(action.modifiers, action.key);
    case "media": return mediaLabel(action.usage);
    case "rapidFire": return `Disparo rápido · ${action.count}× a cada ${action.interval}`;
    case "macro": return macroSummary(macro);
    case "unknown": return `Personalizado (${action.raw.map(b => b.toString(16).padStart(2, "0")).join(" ")})`;
  }
}

export function MouseButtonsPanel() {
  const [buttons, setButtons] = useState<MouseButton[] | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [macro, setMacro] = useState<MacroStep[]>(loadMacro);

  async function run(task: () => Promise<MouseButton[]>, success: string) {
    setBusy(true); setError(""); setNotice("");
    try { setButtons(await task()); setNotice(success); setEditing(null); return true; }
    catch (e) { setError(String(e)); return false; }
    finally { setBusy(false); }
  }

  useEffect(() => {
    setBusy(true);
    invoke<MouseButton[]>("get_mouse_buttons").then(setButtons).catch(e => setError(String(e))).finally(() => setBusy(false));
  }, []);

  async function save(slot: number, action: ButtonAction, steps?: MacroStep[]) {
    const ok = await run(() => invoke<MouseButton[]>("set_mouse_button", { slot, action, macroSteps: steps ?? null }), `${SLOT_NAMES[slot]} salvo no mouse.`);
    if (ok && steps) { setMacro(steps); storeMacro(steps); }
  }

  return <section className="panel mouse-panel">
    <div className="panel-heading">
      <div><p className="eyebrow">AJAZZ AJ179P MC</p><h3>Botões do mouse</h3></div>
      <button className="ghost-button" disabled={busy || !buttons} onClick={() => void run(() => invoke<MouseButton[]>("reset_mouse_buttons"), "Botões restaurados para a fábrica.")}>
        <RotateCcw size={14}/> Restaurar fábrica
      </button>
    </div>
    {error && <p className="mouse-error">{error}</p>}
    {notice && <p className="mouse-notice">{notice}</p>}
    {!buttons ? <p className="activity-empty">{busy ? "Lendo a configuração do mouse…" : "Configuração indisponível."}</p> :
      <div className="button-list">{buttons.map(b =>
        <div className="button-row" key={b.slot}>
          <div className="button-summary">
            <div><strong>{SLOT_NAMES[b.slot] ?? `Botão ${b.slot + 1}`}</strong><span>{actionLabel(b.action, macro)}</span></div>
            <button className="ghost-button" disabled={busy} onClick={() => setEditing(editing === b.slot ? null : b.slot)}>{editing === b.slot ? "Fechar" : "Editar"}</button>
          </div>
          {editing === b.slot && <ButtonEditor button={b} macro={macro} busy={busy} onSave={(a, s) => void save(b.slot, a, s)}/>}
        </div>)}
      </div>}
    <p className="panel-note">As mudanças ficam gravadas no próprio mouse e funcionam mesmo sem o app aberto. O mouse guarda uma macro só: atribuir uma macro a um botão tira a macro do outro.</p>
  </section>;
}

function ButtonEditor({ button, macro, busy, onSave }: { button: MouseButton; macro: MacroStep[]; busy: boolean; onSave: (a: ButtonAction, steps?: MacroStep[]) => void }) {
  const initial = button.action.type === "unknown" ? "default" : button.action.type;
  const [type, setType] = useState<Exclude<ActionType, "unknown">>(initial);
  const [shortcut, setShortcut] = useState(button.action.type === "keyboard" ? { modifiers: button.action.modifiers, key: button.action.key } : { modifiers: 0, key: 0 });
  const [media, setMedia] = useState(button.action.type === "media" ? button.action.usage : MEDIA_KEYS[0].usage);
  const [fire, setFire] = useState(button.action.type === "rapidFire" ? { interval: button.action.interval, count: button.action.count } : { interval: 20, count: 3 });
  const [steps, setSteps] = useState<MacroStep[]>(button.action.type === "macro" ? macro : []);

  function submit() {
    switch (type) {
      case "default": return onSave({ type: "default" });
      case "keyboard": return onSave({ type: "keyboard", ...shortcut });
      case "media": return onSave({ type: "media", usage: media });
      case "rapidFire": return onSave({ type: "rapidFire", ...fire });
      case "macro": return onSave({ type: "macro" }, steps);
    }
  }
  const invalid = (type === "keyboard" && shortcut.key === 0) || (type === "macro" && steps.length === 0);

  return <div className="button-editor">
    <div className="type-tabs">{TYPES.map(t =>
      <button key={t.type} className={type === t.type ? "active" : ""} onClick={() => setType(t.type)}>{t.label}</button>)}
    </div>
    {type === "default" && <p className="editor-hint">O botão volta a fazer o que fazia de fábrica.</p>}
    {type === "keyboard" && <ShortcutCapture value={shortcut} onChange={setShortcut}/>}
    {type === "media" && <label className="field"><Music size={14}/><select value={media} onChange={e => setMedia(Number(e.target.value))}>
      {MEDIA_KEYS.map(m => <option key={m.usage} value={m.usage}>{m.label}</option>)}
    </select></label>}
    {type === "rapidFire" && <div className="field-row">
      <label className="field"><Zap size={14}/>Intervalo<input type="number" min={5} max={255} value={fire.interval} onChange={e => setFire({ ...fire, interval: clamp(e.target.value, 5, 255) })}/></label>
      <label className="field">Repetições<input type="number" min={1} max={255} value={fire.count} onChange={e => setFire({ ...fire, count: clamp(e.target.value, 1, 255) })}/></label>
    </div>}
    {type === "macro" && <MacroRecorder steps={steps} onChange={setSteps}/>}
    <div className="editor-actions">
      <button className="primary-button" disabled={busy || invalid} onClick={submit}><Save size={14}/> {busy ? "Gravando…" : "Salvar no mouse"}</button>
    </div>
  </div>;
}

const clamp = (v: string, min: number, max: number) => Math.min(max, Math.max(min, Math.round(Number(v) || min)));

function ShortcutCapture({ value, onChange }: { value: { modifiers: number; key: number }; onChange: (v: { modifiers: number; key: number }) => void }) {
  const [listening, setListening] = useState(false);
  function onKeyDown(e: KeyboardEvent) {
    e.preventDefault();
    if (isModifierCode(e.code)) { onChange({ modifiers: modifiersFromEvent(e), key: 0 }); return; }
    const key = keyFromEvent(e);
    if (key) { onChange({ modifiers: modifiersFromEvent(e), key: key.usage }); setListening(false); (e.target as HTMLElement).blur(); }
  }
  return <div className="shortcut-capture">
    <div tabIndex={0} className={`capture-box ${listening ? "listening" : ""}`} onFocus={() => setListening(true)} onBlur={() => setListening(false)} onKeyDown={onKeyDown}>
      <Keyboard size={16}/>
      <span>{listening ? (value.key ? shortcutLabel(value.modifiers, value.key) : "Pressione a tecla ou combinação…") : value.key ? shortcutLabel(value.modifiers, value.key) : "Clique aqui e pressione a tecla"}</span>
    </div>
    <label className="field">ou<select value="" onChange={e => onChange({ modifiers: value.modifiers, key: Number(e.target.value) })}>
      <option value="" disabled>tecla especial…</option>
      {SPECIAL_KEYS.map(k => <option key={k.usage} value={k.usage}>{k.label}</option>)}
    </select></label>
  </div>;
}

function MacroRecorder({ steps, onChange }: { steps: MacroStep[]; onChange: (s: MacroStep[]) => void }) {
  const [recording, setRecording] = useState(false);
  const [useTiming, setUseTiming] = useState(true);
  const last = useRef(0);
  const box = useRef<HTMLDivElement>(null);
  const draft = useRef<MacroStep[]>([]);

  const held = useRef(new Set<number>());

  function start() {
    draft.current = []; held.current.clear(); last.current = 0; onChange([]); setRecording(true);
    box.current?.focus();
  }

  function push(kind: MacroStep["kind"], usage: number) {
    const list = draft.current;
    if (list.length >= MAX_MACRO_STEPS) return;
    if (kind === "keyDown") { if (held.current.has(usage)) return; held.current.add(usage); }
    else { if (!held.current.has(usage)) return; held.current.delete(usage); }
    const now = performance.now();
    // A pausa fica no passo anterior: "aperta, espera, solta".
    if (list.length > 0) {
      const gap = Math.round(now - last.current);
      list[list.length - 1] = { ...list[list.length - 1], delayMs: useTiming ? Math.min(Math.max(gap, 10), 5000) : FIXED_DELAY_MS };
    }
    last.current = now;
    list.push({ kind, value: usage, delayMs: FIXED_DELAY_MS });
  }

  function stop() {
    if (!recording) return;
    // Solta o que ficou pressionado, para a macro nunca deixar uma tecla presa
    // (sem herdar o tempo até o clique em "Parar").
    last.current = performance.now() - FIXED_DELAY_MS;
    [...held.current].reverse().forEach(u => push("keyUp", u));
    onChange([...draft.current]);
    setRecording(false);
  }

  function record(e: KeyboardEvent, kind: "keyDown" | "keyUp") {
    if (!recording) return;
    e.preventDefault();
    if (e.repeat) return;
    const key = keyFromEvent(e);
    if (!key) return;
    // Modificadores presos que não chegaram como evento próprio entram antes da tecla.
    if (kind === "keyDown" && !isModifierCode(e.code)) heldModifierUsages(e).forEach(u => push("keyDown", u));
    push(kind, key.usage);
    onChange([...draft.current]);
  }

  return <div className="macro-recorder">
    <div ref={box} tabIndex={0} className={`capture-box ${recording ? "listening recording" : ""}`} onKeyDown={e => record(e, "keyDown")} onKeyUp={e => record(e, "keyUp")}
      // Só para quando o foco sai da caixa (não ao passar para o botão dentro dela).
      onBlur={e => { if (!e.currentTarget.contains(e.relatedTarget as Node)) stop(); }}>
      <ListVideo size={16}/>
      <span>{recording ? "Gravando: digite a sequência…" : steps.length ? `${steps.length} passos gravados` : "Nenhuma macro gravada"}</span>
      {recording
        ? <button className="ghost-button" onMouseDown={e => e.preventDefault()} onClick={stop}><Square size={12}/> Parar</button>
        : <button className="ghost-button" onMouseDown={e => e.preventDefault()} onClick={start}><Circle size={12} className="rec-dot"/> Gravar</button>}
    </div>
    {steps.length > 0 && <div className="macro-steps">{steps.map((s, i) =>
      <span key={i} className={`macro-step ${s.kind}`} title={`pausa de ${s.delayMs} ms`}>{s.kind === "keyDown" ? "↓" : "↑"} {keyLabel(s.value)}</span>)}
    </div>}
    <label className="check"><input type="checkbox" checked={useTiming} onChange={e => setUseTiming(e.target.checked)}/> Usar os tempos gravados (sem isso, {FIXED_DELAY_MS} ms entre teclas)</label>
  </div>;
}
