import { angular } from "@angular-wave/angular.ts";
import { score_term as scoreTerm } from "./search-policy.js";

const normalize = (value) => value.toLocaleLowerCase().normalize("NFKD");
const termsFor = (value) => normalize(value).split(/\s+/u).filter(Boolean);

const score = (document, terms) => {
  const title = normalize(document.title || "");
  const description = normalize(document.description || "");
  const section = normalize(document.section || "");
  const content = normalize(document.content || "");
  let total = 0;
  for (const term of terms) {
    const termScore = scoreTerm(title, description, section, content, term);
    if (termScore === 0) return 0;
    total += termScore;
  }
  return total;
};

const render = (results, documents, terms) => {
  results.replaceChildren();
  if (terms.length === 0) return;
  const matches = documents
    .map((document) => ({ document, score: score(document, terms) }))
    .filter((match) => match.score > 0)
    .sort((left, right) => right.score - left.score ||
      left.document.title.localeCompare(right.document.title))
    .slice(0, 10);
  for (const { document } of matches) {
    const row = window.document.createElement("li");
    row.dataset.slot = "command-item";
    const link = window.document.createElement("a");
    link.href = new URL(document.url, window.document.baseURI).href;
    link.textContent = document.title;
    row.append(link);
    if (document.description) {
      const summary = window.document.createElement("p");
      summary.textContent = document.description;
      row.append(summary);
    }
    results.append(row);
  }
  if (matches.length === 0) {
    const row = window.document.createElement("li");
    row.dataset.slot = "command-empty";
    row.textContent = "No results";
    results.append(row);
  }
};

const start = async (root) => {
  const input = root.querySelector("[data-terl-docs-search-input]");
  const results = root.querySelector("[data-terl-docs-search-results]");
  if (!input || !results) return;
  const indexUrl = new URL(root.dataset.index || "search-index.json", document.baseURI);
  const response = await fetch(indexUrl);
  if (!response.ok) throw new Error(`search index returned ${response.status}`);
  const index = await response.json();
  if (index.version !== 1 || !Array.isArray(index.documents)) {
    throw new Error("unsupported search index schema");
  }
  input.addEventListener("input", () => render(results, index.documents, termsFor(input.value)));
  root.addEventListener("submit", (event) => event.preventDefault());
  root.dataset.searchRuntime = "angular-ts";
};

angular.module("terlDocs", []).directive("terlDocsSearch", () => ({
  restrict: "A",
  link(_scope, root) {
    start(root).catch(() => {
      root.dataset.searchState = "unavailable";
    });
  },
}));

if (typeof globalThis === "object") {
  globalThis.angular = angular;
}

const init = () => angular.init(document);
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init, { once: true });
} else {
  init();
}
