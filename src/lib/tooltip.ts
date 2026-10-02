// Lightweight styled tooltip: `<img use:tip={{ title, meta, body }} />`.
// One shared floating element; shows on hover and keyboard focus.

export type TipContent =
  | string
  | { title: string; meta?: string; body?: string }
  | null
  | undefined
  | false;

let el: HTMLDivElement | null = null;
let owner: HTMLElement | null = null;

function ensure(): HTMLDivElement {
  if (!el) {
    el = document.createElement("div");
    el.className = "tip";
    el.setAttribute("role", "tooltip");
    document.body.appendChild(el);
  }
  return el;
}

function line(cls: string, text: string) {
  const d = document.createElement("div");
  d.className = cls;
  d.textContent = text;
  return d;
}

function show(target: HTMLElement, content: TipContent) {
  if (!content) return;
  const t = ensure();
  const c = typeof content === "string" ? { title: content } : content;
  t.replaceChildren(line("tip-title", c.title));
  if (c.meta) t.append(line("tip-meta", c.meta));
  if (c.body) t.append(line("tip-body", c.body));
  owner = target;
  t.style.visibility = "hidden";
  t.style.display = "block";

  const r = target.getBoundingClientRect();
  const tw = t.offsetWidth;
  const th = t.offsetHeight;
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  let top = r.top - th - 8;
  if (top < 6) top = Math.min(r.bottom + 8, vh - th - 6);
  const left = Math.max(6, Math.min(r.left + r.width / 2 - tw / 2, vw - tw - 6));
  t.style.top = `${Math.round(top)}px`;
  t.style.left = `${Math.round(left)}px`;
  t.style.visibility = "visible";
}

function hide(target?: HTMLElement) {
  if (el && (!target || owner === target)) {
    el.style.display = "none";
    owner = null;
  }
}

export function tip(node: HTMLElement, content: TipContent) {
  let c = content;
  const enter = () => show(node, c);
  const leave = () => hide(node);
  node.addEventListener("mouseenter", enter);
  node.addEventListener("mouseleave", leave);
  node.addEventListener("focusin", enter);
  node.addEventListener("focusout", leave);
  node.addEventListener("pointerdown", leave);
  return {
    update(n: TipContent) {
      c = n;
      if (owner === node) show(node, c);
    },
    destroy() {
      node.removeEventListener("mouseenter", enter);
      node.removeEventListener("mouseleave", leave);
      node.removeEventListener("focusin", enter);
      node.removeEventListener("focusout", leave);
      node.removeEventListener("pointerdown", leave);
      hide(node);
    },
  };
}
