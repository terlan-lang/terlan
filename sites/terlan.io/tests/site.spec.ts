import { expect, test } from "@playwright/test";

test("renders the documentation shell with angular.css components", async ({ page }) => {
  await page.goto("/docs/getting-started/");

  await expect(page).toHaveTitle("Getting started · Terlan");
  await expect(page.getByRole("heading", { level: 1, name: "Getting started" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Documentation navigation" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toContainText(
    "Home/Docs/Getting started",
  );

  const activeLink = page.getByRole("link", { name: "Getting started", exact: true }).first();
  await expect(activeLink).toHaveAttribute("data-slot", "sidebar-menu-sub-button");
  await expect(activeLink).toHaveAttribute("data-active", "true");
  await expect(activeLink).toHaveCSS("background-color", "rgb(228, 241, 236)");
});

test("searches the generated local index through the Terlan policy", async ({ page }) => {
  const clientResponse = page.waitForResponse((response) =>
    response.url().endsWith("/assets/terl-docs/search.js"),
  );
  await page.goto("/docs/");
  await expect((await clientResponse).ok()).toBe(true);

  const searchForm = page.getByRole("search");
  await expect(searchForm).toHaveAttribute("data-search-runtime", "angular-ts");

  const search = page.getByRole("searchbox", { name: "Search documentation" });
  await search.fill("language modules");

  const result = page.locator("[data-terl-docs-search-results]").getByRole("link", {
    name: "Language guide",
  });
  await expect(result).toBeVisible();
  await expect(result).toHaveAttribute("href", /\/docs\/language\/$/);
  await expect(result.locator("..")).toHaveAttribute("data-slot", "command-item");
});

test("keeps keyboard focus visible", async ({ page }) => {
  await page.goto("/docs/getting-started/");
  await page.keyboard.press("Tab");

  const skipLink = page.getByRole("link", { name: "Skip to content" });
  await expect(skipLink).toBeFocused();
  await expect(skipLink).toBeVisible();
  await expect(skipLink).toHaveAttribute("href", "docs/getting-started/#main-content");
  await skipLink.click();
  await expect(page).toHaveURL(/\/docs\/getting-started\/#main-content$/);
});

test("redirects route aliases to the canonical documentation page", async ({ page }) => {
  await page.goto("/start/");

  await expect(page).toHaveURL(/\/docs\/getting-started\/$/);
  await expect(page.getByRole("heading", { level: 1, name: "Getting started" })).toBeVisible();
});

test("does not publish draft or scheduled posts past the production cutoff", async ({ page }) => {
  const response = await page.goto("/blog/search-roadmap/");

  expect(response?.status()).toBe(404);
  await page.goto("/docs/");
  const search = page.getByRole("searchbox", { name: "Search documentation" });
  await search.fill("Search roadmap notes");
  await expect(
    page.locator("[data-terl-docs-search-results]").getByRole("link", {
      name: "Search roadmap notes",
    }),
  ).toHaveCount(0);

  const scheduledResponse = await page.goto("/blog/compiler-notes/");
  expect(scheduledResponse?.status()).toBe(404);
  const aliasResponse = await page.goto("/upcoming/");
  expect(aliasResponse?.status()).toBe(404);
});

test("links the page table of contents to stable heading anchors", async ({ page }) => {
  await page.goto("/docs/language/");

  const toc = page.getByRole("navigation", { name: "On this page" });
  await expect(toc).toBeVisible();
  const sectionLink = toc.getByRole("link", { name: "A small module" });
  await expect(sectionLink).toHaveAttribute("href", "docs/language/#a-small-module");

  await sectionLink.click();
  await expect(page).toHaveURL(/\/docs\/language\/#a-small-module$/);
  await expect(page.locator("#a-small-module")).toHaveAttribute("tabindex", "-1");
});

test("renders generated blog archive, tag, and author collections", async ({ page }) => {
  await page.goto("/blog/archive/");

  await expect(page).toHaveTitle("Blog archive · Terlan");
  await expect(page.getByRole("heading", { level: 1, name: "Blog archive" })).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Introducing the Terlan documentation stack" }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "compiler" })).toHaveAttribute(
    "href",
    "blog/tags/compiler/",
  );

  await page.goto("/blog/tags/compiler/");
  await expect(page.getByRole("heading", { level: 1, name: "Posts tagged “compiler”" })).toBeVisible();

  await page.goto("/blog/authors/terlan-team/");
  await expect(page.getByRole("heading", { level: 1, name: "Posts by Terlan team" })).toBeVisible();
});
