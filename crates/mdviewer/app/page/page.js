// The rendered view's page: shows the HTML the app sends, and tells the app what it shows and what
// the reader does on it. Every message is JSON; `views/page.tsx` is the other end.
(() => {
  "use strict";

  const content = document.getElementById("content");
  const send = (message) => window.nsh.postMessage(JSON.stringify(message));

  const SCHEME = /^[a-z][a-z0-9+.-]*:/i;
  const BLOCKS = ":is(h1, h2, h3, h4, h5, h6, p, ul, ol, li, blockquote, pre, table, tr, hr, div, details, section, dl)[data-sourcepos]";
  // Text in one of these is searched as one run, across the formatting inside it.
  const RUNS = "p, li, h1, h2, h3, h4, h5, h6, td, th, pre, dt, dd, summary, blockquote, div, section, details, figcaption, main";
  // As many as the app's search finds.
  const MOST_FOUND = 10000;
  // Must agree with Raw's search: an end in punctuation is a word's edge by itself.
  const WORD_BEFORE = /[\p{L}\p{N}_]$/u;
  const WORD_AFTER = /^[\p{L}\p{N}_]/u;
  const MODIFIERS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock"]);
  // Keys that stay the page's: moving through it, copying and selecting.
  const OWN_KEYS = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", "Home", "End", " ", "Tab", "Enter"]);

  // A picture of the project, by the id it was asked for with.
  const pictures = new Map();
  let asked = 0;
  const watcher = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        watcher.unobserve(entry.target);
        const id = String(asked++);
        const picture = waiting.get(entry.target);
        pictures.set(id, entry.target);
        send({ type: "wantPicture", id, src: picture });
      }
    },
    { rootMargin: "800px 0px" },
  );
  // The project's pictures not asked for yet, and their paths in the project.
  let waiting = new Map();

  // The search's matches on the page, in its order, and the last time the app asked to go to one.
  let found = [];
  let revealed = 0;

  const lineOf = (place) => parseInt(place, 10) - 1;

  function show(html, top) {
    const holder = document.createElement("template");
    // A template's content is inert: nothing in it loads until it is shown.
    holder.innerHTML = html;
    const shown = new Map();
    for (const picture of holder.content.querySelectorAll("img")) {
      const src = picture.getAttribute("src") ?? "";
      if (src === "" || SCHEME.test(src)) continue;
      picture.removeAttribute("src");
      picture.classList.add("waiting");
      shown.set(picture, src);
    }
    for (const block of holder.content.querySelectorAll("pre:not(.front-matter)")) {
      const wrapper = document.createElement("div");
      wrapper.className = "code-block";
      block.replaceWith(wrapper);
      const copy = document.createElement("button");
      copy.className = "copy";
      copy.type = "button";
      copy.textContent = "Copy";
      wrapper.append(block, copy);
    }
    watcher.disconnect();
    pictures.clear();
    waiting = shown;
    found = [];
    CSS.highlights.clear();
    const keep = window.scrollY;
    content.replaceChildren(holder.content);
    for (const picture of shown.keys()) watcher.observe(picture);
    window.scrollTo({ top: top ?? keep, behavior: "instant" });
    reportLayout();
    reportScroll();
  }

  function theme(vars, dark) {
    const root = document.documentElement;
    for (const [name, value] of Object.entries(vars)) {
      if (name.startsWith("--")) root.style.setProperty(name, String(value));
    }
    root.classList.toggle("dark", Boolean(dark));
  }

  function blocks() {
    const found = [];
    for (const block of content.querySelectorAll(BLOCKS)) {
      const [start, end] = block.dataset.sourcepos.split("-");
      const box = block.getBoundingClientRect();
      let depth = 0;
      for (let around = block.parentElement; around && around !== content; around = around.parentElement) {
        if (around.matches(BLOCKS)) depth += 1;
      }
      found.push({ line: lineOf(start), endLine: lineOf(end), top: box.top + window.scrollY, height: box.height, kind: block.localName, depth });
    }
    return found;
  }

  let layoutPending = false;
  function reportLayout() {
    if (layoutPending) return;
    layoutPending = true;
    requestAnimationFrame(() => {
      layoutPending = false;
      send({ type: "layout", height: document.documentElement.scrollHeight, blocks: blocks() });
      if (found.length > 0) reportFound();
    });
  }

  function reportScroll() {
    send({ type: "scroll", y: window.scrollY, viewport: window.innerHeight, content: document.documentElement.scrollHeight });
  }

  /** The block that shows `line`, from 0: the heading that starts on it, else the innermost block around it. */
  function blockAt(line) {
    let best = null;
    let span = Infinity;
    for (const block of content.querySelectorAll(BLOCKS)) {
      const [start, end] = block.dataset.sourcepos.split("-");
      const first = lineOf(start);
      const last = lineOf(end);
      if (line < first || line > last) continue;
      if (first === line && /^h[1-6]$/.test(block.localName)) return block;
      if (last - first <= span) {
        best = block;
        span = last - first;
      }
    }
    return best;
  }

  function scrollToBlock(block, above, animated) {
    const top = block.getBoundingClientRect().top + window.scrollY - above;
    window.scrollTo({ top: Math.max(0, top), behavior: animated ? "smooth" : "instant" });
  }

  function picture(id, data, problem) {
    const shown = pictures.get(id);
    if (!shown) return;
    pictures.delete(id);
    shown.classList.remove("waiting");
    if (typeof data === "string" && data.startsWith("data:")) {
      shown.src = data;
    } else {
      shown.classList.add("missing");
      shown.title = String(problem ?? "");
    }
  }

  function wholeWordAt(text, start, end) {
    const before = WORD_BEFORE.test(text.slice(Math.max(0, start - 2), start));
    const first = WORD_AFTER.test(text.slice(start, end));
    const last = WORD_BEFORE.test(text.slice(start, end));
    const after = WORD_AFTER.test(text.slice(end, end + 2));
    return (!before || !first) && (!after || !last);
  }

  /** The page's text in runs, each the text nodes of one block in a row, with where each node starts in it. */
  function runs() {
    const all = [];
    let run = null;
    const walker = document.createTreeWalker(content, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const parent = node.parentElement;
      if (!parent || parent.closest(".copy")) continue;
      const holder = parent.closest(RUNS);
      if (!run || run.holder !== holder) {
        run = { holder, nodes: [], starts: [], text: "" };
        all.push(run);
      }
      run.nodes.push(node);
      run.starts.push(run.text.length);
      run.text += node.data;
    }
    return all;
  }

  /** The node of `run` and the offset in it of `at`; an `end` at a node's end stays in that node. */
  function pointOf(run, at, end) {
    let index = run.nodes.length - 1;
    for (let next = 1; next < run.nodes.length; next++) {
      if (end ? run.starts[next] >= at : run.starts[next] > at) {
        index = next - 1;
        break;
      }
    }
    return [run.nodes[index], at - run.starts[index]];
  }

  function rangeOf(run, start, end) {
    const range = new Range();
    range.setStart(...pointOf(run, start, false));
    range.setEnd(...pointOf(run, end, true));
    return range;
  }

  /** The page's match for the app's `current`th: in the block that shows its line, as many in as the app found before it there. */
  function currentOf(lines, current) {
    const line = lines[current];
    if (line === undefined) return null;
    const block = blockAt(line);
    if (!block) return null;
    const [start, end] = block.dataset.sourcepos.split("-");
    const first = lineOf(start);
    const last = lineOf(end);
    let nth = 0;
    for (let each = 0; each < current; each++) {
      if (lines[each] >= first && lines[each] <= last) nth += 1;
    }
    const inside = found.filter((range) => block.contains(range.startContainer));
    return inside[Math.min(nth, inside.length - 1)] ?? null;
  }

  function reportFound() {
    send({ type: "found", count: found.length, places: found.map((range) => range.getBoundingClientRect().top + window.scrollY) });
  }

  /** Highlights what the app's search finds in the page's text, across the formatting in a block, and the current match stronger. */
  function find(message) {
    CSS.highlights.clear();
    found = [];
    let pattern = null;
    try {
      if (message.source) pattern = new RegExp(String(message.source), String(message.flags ?? ""));
    } catch {}
    if (pattern?.global) {
      search: for (const run of runs()) {
        if (run.holder?.checkVisibility?.() === false) continue;
        for (const match of run.text.matchAll(pattern)) {
          const start = match.index;
          const end = start + match[0].length;
          if (end <= start || (message.wholeWord && !wholeWordAt(run.text, start, end))) continue;
          found.push(rangeOf(run, start, end));
          if (found.length >= MOST_FOUND) break search;
        }
      }
    }
    const lines = Array.isArray(message.lines) ? message.lines.map(Number) : [];
    const index = Number(message.current);
    const current = currentOf(lines, index);
    const all = new Highlight();
    for (const range of found) if (range !== current) all.add(range);
    CSS.highlights.set("found", all);
    if (current) {
      const strong = new Highlight(current);
      strong.priority = 1;
      CSS.highlights.set("found-current", strong);
    }
    reportFound();

    const reveal = Number(message.reveal) || 0;
    if (reveal === revealed || lines[index] === undefined) return;
    revealed = reveal;
    const above = Number(message.above) || 0;
    if (current) {
      window.scrollTo({ top: Math.max(0, current.getBoundingClientRect().top + window.scrollY - above), behavior: "smooth" });
    } else {
      const block = blockAt(lines[index]);
      if (block) scrollToBlock(block, above, true);
    }
  }

  window.addEventListener("message", (event) => {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    switch (message?.type) {
      case "show":
        show(String(message.html ?? ""), typeof message.top === "number" ? message.top : null);
        break;
      case "theme":
        theme(message.vars ?? {}, message.dark);
        break;
      case "scrollTo":
        window.scrollTo({ top: Number(message.y) || 0, behavior: message.animated ? "smooth" : "instant" });
        break;
      case "goToLine": {
        const block = blockAt(Number(message.line));
        if (block) scrollToBlock(block, Number(message.above) || 0, message.animated !== false);
        break;
      }
      case "find":
        find(message);
        break;
      case "picture":
        picture(String(message.id), message.data, message.problem);
        break;
      case "selectAll":
        window.getSelection()?.selectAllChildren(content);
        break;
    }
  });

  function follow(event, link) {
    event.preventDefault();
    const url = link.getAttribute("href") ?? "";
    if (url.startsWith("#")) {
      let id = url.slice(1);
      try {
        id = decodeURIComponent(id);
      } catch {}
      const target = document.getElementById(id);
      if (target && content.contains(target)) {
        scrollToBlock(target, 12, true);
        return;
      }
    }
    send({ type: "link", url, control: event.ctrlKey || event.metaKey || event.button === 1 });
  }

  document.addEventListener("click", (event) => {
    const copy = event.target.closest?.(".copy");
    if (copy) {
      send({ type: "copy", text: copy.parentElement.querySelector("pre")?.innerText ?? "" });
      copy.textContent = "Copied";
      setTimeout(() => (copy.textContent = "Copy"), 1200);
      return;
    }
    const link = event.target.closest?.("a[href]");
    if (link) follow(event, link);
  });

  document.addEventListener("auxclick", (event) => {
    const link = event.target.closest?.("a[href]");
    if (link && event.button === 1) follow(event, link);
  });

  // The app's own menu in place of the browser's.
  document.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    const link = event.target.closest?.("a[href]")?.getAttribute("href") ?? "";
    send({ type: "menu", x: event.clientX, y: event.clientY, link, selection: String(window.getSelection() ?? "") });
  });

  // The app's shortcuts go to the app; the browser's own (reload, print, find, zoom) go nowhere.
  document.addEventListener(
    "keydown",
    (event) => {
      const key = event.key;
      const control = event.ctrlKey || event.metaKey;
      if (MODIFIERS.has(key)) return;
      if (!control && !event.altKey && OWN_KEYS.has(key)) return;
      if (control && !event.altKey && !event.shiftKey && (key === "c" || key === "a" || key === "Insert")) return;
      const shortcut = control || event.altKey || key === "Escape" || /^F\d{1,2}$/.test(key);
      if (!shortcut) return;
      event.preventDefault();
      send({ type: "key", key, code: event.code, control, shift: event.shiftKey, alt: event.altKey });
    },
    true,
  );

  window.addEventListener(
    "wheel",
    (event) => {
      if (event.ctrlKey) event.preventDefault();
    },
    { passive: false },
  );

  // A file dropped here would replace the page.
  for (const name of ["dragover", "drop"]) document.addEventListener(name, (event) => event.preventDefault());

  window.addEventListener("scroll", reportScroll, { passive: true });
  new ResizeObserver(reportLayout).observe(content);
  window.addEventListener("resize", () => {
    reportLayout();
    reportScroll();
  });
  content.addEventListener("load", reportLayout, true);
  document.fonts?.ready.then(reportLayout);

  send({ type: "ready" });
})();
