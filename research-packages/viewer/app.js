const packageRoot = "../packages/from-agi-to-asi-2606-12683";
const files = {
  concepts: `${packageRoot}/trace/concepts.json`,
  claims: `${packageRoot}/evidence/claims.json`,
  manifest: `${packageRoot}/evidence/manifest.json`,
};

const state = {
  concepts: [],
  claims: [],
  manifest: null,
  selectedId: null,
  activeClaim: null,
  positions: new Map(),
  animated: false,
};

const colors = {
  active: "#70c7da",
  proven: "#6dd3a8",
  abandoned: "#e46f63",
  dead_end: "#e46f63",
  strategy: "#e0b45d",
  approach: "#70c7da",
};

Promise.all([fetchJson(files.concepts), fetchJson(files.claims), fetchJson(files.manifest)])
  .then(([concepts, claims, manifest]) => {
    state.concepts = concepts.nodes;
    state.claims = claims;
    state.manifest = manifest;
    state.selectedId = state.concepts[0]?.id;
    render();
  })
  .catch((error) => {
    document.getElementById("paper-title").textContent = "Trace failed to load";
    document.getElementById("selected-summary").textContent = error.message;
  });

async function fetchJson(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Unable to load ${path}: ${response.status}`);
  }
  return response.json();
}

function render() {
  document.getElementById("paper-title").textContent = state.manifest.title;
  renderMap();
  renderClaims();
  renderInspector(state.concepts.find((node) => node.id === state.selectedId));
  renderProof();
  bindImages();
  bindModes();
  animateInitialView();
}

function renderMap() {
  const svg = document.getElementById("trace-map");
  svg.textContent = "";
  const width = 960;
  const height = 680;
  const center = { x: width / 2, y: height / 2 };
  const root = state.concepts.find((node) => !node.parent) ?? state.concepts[0];
  const children = state.concepts.filter((node) => node.id !== root.id);
  state.positions.clear();
  state.positions.set(root.id, center);

  children.forEach((node, index) => {
    const angle = -Math.PI / 2 + (index / children.length) * Math.PI * 2;
    const radius = node.kind === "dead_end" ? 250 : 225 + (index % 2) * 34;
    state.positions.set(node.id, {
      x: center.x + Math.cos(angle) * radius,
      y: center.y + Math.sin(angle) * radius,
    });
  });

  const defs = svgEl("defs");
  defs.appendChild(filterGlow());
  svg.appendChild(defs);

  state.concepts
    .filter((node) => node.parent)
    .forEach((node) => {
      const from = state.positions.get(node.parent);
      const to = state.positions.get(node.id);
      const path = svgEl("path", {
        class: `edge ${state.selectedId === node.id ? "active" : ""}`,
        d: curve(from, to),
      });
      svg.appendChild(path);
      addSignal(svg, from, to, node);
    });

  if (state.activeClaim) {
    drawClaimLink(svg, center);
  }

  state.concepts.forEach((node) => {
    const pos = state.positions.get(node.id);
    const group = svgEl("g", {
      class: `node ${state.selectedId === node.id ? "selected" : ""}`,
      transform: `translate(${pos.x} ${pos.y})`,
      tabindex: "0",
      role: "button",
    });
    group.addEventListener("click", () => selectNode(node.id));
    group.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") selectNode(node.id);
    });

    group.appendChild(svgEl("circle", {
      r: node.id === root.id ? 25 : 17,
      fill: nodeColor(node),
      filter: "url(#glow)",
    }));
    group.appendChild(svgEl("text", {
      x: node.id === root.id ? 0 : pos.x > center.x ? 27 : -27,
      y: node.id === root.id ? 45 : 5,
      "text-anchor": node.id === root.id ? "middle" : pos.x > center.x ? "start" : "end",
    }, label(node.description, 28)));
    svg.appendChild(group);
  });

  animateMapPulse();
  animateGraphContinuously();
}

function addSignal(svg, from, to, node) {
  const dot = svgEl("circle", {
    class: "signal",
    r: node.kind === "dead_end" ? 4.4 : 3.6,
    cx: from.x,
    cy: from.y,
    fill: nodeColor(node),
    "data-from-x": from.x,
    "data-from-y": from.y,
    "data-to-x": to.x,
    "data-to-y": to.y,
  });
  svg.appendChild(dot);
}

function drawClaimLink(svg, center) {
  const claimIndex = Math.max(0, state.claims.findIndex((claim) => claim.id === state.activeClaim));
  const target = state.concepts[(claimIndex % (state.concepts.length - 1)) + 1];
  const to = state.positions.get(target.id);
  svg.appendChild(svgEl("path", {
    class: "edge claim-link active",
    d: curve(center, to),
  }));
}

function renderClaims() {
  const wrap = document.getElementById("claims");
  wrap.textContent = "";
  document.getElementById("claim-count").textContent =
    `${state.claims.length} claims, ${state.manifest.review.falsifiable_fraction * 100}% falsifiable`;

  state.claims.forEach((claim) => {
    const button = document.createElement("button");
    button.className = `claim ${state.activeClaim === claim.id ? "active" : ""}`;
    button.type = "button";
    button.innerHTML = `
      <strong>${escapeHtml(claim.id)}</strong>
      <p>${escapeHtml(claim.text)}</p>
      <span>${escapeHtml(claim.scope ?? "No scope")} · ${escapeHtml(claim.evidence_refs.join(", "))}</span>
    `;
    button.addEventListener("click", () => selectClaim(claim.id));
    wrap.appendChild(button);
  });
}

function renderInspector(node) {
  if (!node) return;
  document.getElementById("selected-mark").style.background = nodeColor(node);
  document.getElementById("selected-kind").textContent = `${node.kind} · ${node.status}`;
  document.getElementById("selected-title").textContent = node.description;
  document.getElementById("selected-summary").textContent =
    node.outcome_summary || node.dead_end_reason || "No summary recorded.";

  const facts = [
    ["Concept id", node.id],
    ["Parent", node.parent ?? "root"],
    ["Provenance", node.provenance],
  ];
  if (node.dead_end_reason) facts.push(["Dead-end reason", node.dead_end_reason]);
  document.getElementById("selected-facts").innerHTML = facts
    .map(([term, value]) => `<div><dt>${escapeHtml(term)}</dt><dd>${escapeHtml(value)}</dd></div>`)
    .join("");
}

function renderProof() {
  document.getElementById("review-score").style.width = `${state.manifest.review.score}%`;
  const chain = state.manifest.chain_reference;
  const proof = [
    ["Review", `${state.manifest.review.verdict} · ${state.manifest.review.score}/100`],
    ["Source hash", state.manifest.source_hash],
    ["Package root", state.manifest.root_hash],
    ["Signature", state.manifest.author_signature],
    ["Chain", `${chain.network} · block ${chain.block_number} · finalized ${chain.finalized}`],
    ["Transaction", chain.transaction_hash],
    ["Created", state.manifest.created_at],
    ["Reviewer", state.manifest.reviewer],
  ];
  document.getElementById("proof-grid").innerHTML = proof
    .map(([term, value]) => `
      <div class="proof-item">
        <dt>${escapeHtml(term)}</dt>
        <dd><code>${escapeHtml(value)}</code></dd>
      </div>
    `)
    .join("");
}

function bindModes() {
  document.querySelectorAll(".mode").forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelectorAll(".mode").forEach((item) => item.classList.remove("active"));
      button.classList.add("active");
      document.body.className = `${button.dataset.mode}-mode`;
    });
  });
}

function bindImages() {
  document.querySelectorAll(".image-panel").forEach((panel) => {
    panel.addEventListener("click", () => {
      const focus = panel.dataset.focus;
      if (focus) selectNode(focus);
      if (window.anime) {
        anime({
          targets: panel.querySelector("img"),
          scale: [1.08, 1.02],
          duration: 720,
          easing: "easeOutExpo",
        });
      }
    });
  });
}

function selectNode(id) {
  state.selectedId = id;
  state.activeClaim = null;
  renderMap();
  renderClaims();
  renderInspector(state.concepts.find((node) => node.id === id));
}

function selectClaim(id) {
  state.activeClaim = id;
  const claimIndex = state.claims.findIndex((claim) => claim.id === id);
  const concept = state.concepts[(claimIndex % (state.concepts.length - 1)) + 1];
  state.selectedId = concept.id;
  renderMap();
  renderClaims();
  renderInspector(concept);
}

function animateInitialView() {
  if (state.animated || !window.anime) return;
  state.animated = true;
  anime.timeline({ easing: "easeOutExpo" })
    .add({
      targets: ".topbar > *, .proof-panel",
      translateY: [-18, 0],
      opacity: [0, 1],
      duration: 720,
      delay: anime.stagger(80),
    })
    .add({
      targets: ".image-panel",
      translateY: [22, 0],
      opacity: [0, 1],
      duration: 900,
      delay: anime.stagger(120),
    }, "-=420")
    .add({
      targets: ".node",
      scale: [0.72, 1],
      opacity: [0, 1],
      duration: 760,
      delay: anime.stagger(45),
    }, "-=520")
    .add({
      targets: ".claim",
      translateY: [18, 0],
      opacity: [0, 1],
      duration: 620,
      delay: anime.stagger(55),
    }, "-=520");

  anime({
    targets: ".hero-image img",
    scale: [1.04, 1.1],
    translateX: [0, -18],
    duration: 14000,
    direction: "alternate",
    loop: true,
    easing: "easeInOutSine",
  });

  anime({
    targets: ".image-stack img",
    scale: [1.02, 1.08],
    translateY: [0, -10],
    duration: 11000,
    direction: "alternate",
    loop: true,
    delay: anime.stagger(900),
    easing: "easeInOutSine",
  });

  anime({
    targets: ".motion-orbit span",
    rotate: [0, 360],
    scale: [0.92, 1.08],
    opacity: [0.18, 0.58],
    duration: 9000,
    direction: "alternate",
    loop: true,
    delay: anime.stagger(650),
    easing: "easeInOutSine",
  });
}

function animateMapPulse() {
  if (!window.anime) return;
  anime.remove(".node.selected circle");
  anime({
    targets: ".node.selected circle",
    scale: [1, 1.12],
    duration: 1300,
    direction: "alternate",
    loop: true,
    easing: "easeInOutSine",
  });
}

function animateGraphContinuously() {
  if (!window.anime) return;
  anime.remove(".edge");
  anime.remove(".signal");
  anime.remove(".node:not(.selected)");
  anime.remove(".claim");

  anime({
    targets: ".edge",
    strokeDashoffset: [760, 0],
    opacity: [0.2, 0.78],
    duration: 1800,
    delay: anime.stagger(90),
    easing: "easeOutExpo",
  });

  anime({
    targets: ".signal",
    cx: (el) => [Number(el.dataset.fromX), Number(el.dataset.toX)],
    cy: (el) => [Number(el.dataset.fromY), Number(el.dataset.toY)],
    opacity: [0, 1, 0],
    scale: [0.4, 1.45, 0.4],
    duration: 2600,
    delay: anime.stagger(210),
    loop: true,
    easing: "easeInOutSine",
  });

  anime({
    targets: ".node:not(.selected)",
    translateY: [
      { value: -7, duration: 1800 },
      { value: 7, duration: 1800 },
      { value: 0, duration: 1800 },
    ],
    rotate: [
      { value: -1.4, duration: 1800 },
      { value: 1.4, duration: 1800 },
      { value: 0, duration: 1800 },
    ],
    delay: anime.stagger(160),
    loop: true,
    easing: "easeInOutSine",
  });

  anime({
    targets: ".claim",
    boxShadow: [
      "0 0 0 rgba(224,180,93,0)",
      "0 0 24px rgba(224,180,93,.12)",
      "0 0 0 rgba(224,180,93,0)",
    ],
    duration: 4200,
    delay: anime.stagger(340),
    loop: true,
    easing: "easeInOutSine",
  });
}

function nodeColor(node) {
  if (node.kind === "dead_end") return colors.dead_end;
  if (node.status === "proven") return colors.proven;
  if (node.description.toLowerCase().includes("friction")) return colors.amber;
  return colors[node.kind] || colors.active;
}

function curve(from, to) {
  const midX = (from.x + to.x) / 2;
  const midY = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const bend = 0.16;
  const cx = midX - dy * bend;
  const cy = midY + dx * bend;
  return `M ${from.x} ${from.y} Q ${cx} ${cy} ${to.x} ${to.y}`;
}

function filterGlow() {
  const filter = svgEl("filter", { id: "glow", x: "-80%", y: "-80%", width: "260%", height: "260%" });
  filter.appendChild(svgEl("feGaussianBlur", { stdDeviation: "5", result: "blur" }));
  const merge = svgEl("feMerge");
  merge.appendChild(svgEl("feMergeNode", { in: "blur" }));
  merge.appendChild(svgEl("feMergeNode", { in: "SourceGraphic" }));
  filter.appendChild(merge);
  return filter;
}

function svgEl(name, attrs = {}, text = "") {
  const element = document.createElementNS("http://www.w3.org/2000/svg", name);
  Object.entries(attrs).forEach(([key, value]) => element.setAttribute(key, value));
  if (text) element.textContent = text;
  return element;
}

function label(text, max) {
  return text.length > max ? `${text.slice(0, max - 1)}...` : text;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}
