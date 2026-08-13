import { expect, test } from "@playwright/test";
import path from "node:path";

test("Terlan-owned todo runs through AngularTS", async ({ page }) => {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];
  const terlanModules: string[] = [];

  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("request", (request) => {
    if (request.url().endsWith("/terlan/angular/Todo.js")) {
      terlanModules.push(request.url());
    }
  });

  const angularRoot = path.resolve(process.cwd(), "../..");
  await page.route("http://terlan.test/**", async (route) => {
    const requestPath = decodeURIComponent(new URL(route.request().url()).pathname);
    const filePath = path.resolve(angularRoot, `.${requestPath}`);
    if (!filePath.startsWith(`${angularRoot}${path.sep}`)) {
      await route.abort("blockedbyclient");
      return;
    }
    await route.fulfill({ path: filePath });
  });

  await page.goto("integrations/terlan/examples/todo/index.html");
  await expect(page.getByRole("heading", { name: "Terlan Angular Todo" })).toBeVisible();
  const heading = page.getByRole("heading", { name: "Terlan Angular Todo" });
  await expect(heading).toHaveAttribute("data-terlan-directive", "mounted");
  await heading.dispatchEvent("terlan:probe");
  await expect(heading).toHaveAttribute("data-terlan-callback", "invoked");
  await expect(page.getByText("No todos")).toBeVisible();
  expect(terlanModules).toHaveLength(1);

  const draft = page.getByLabel("New todo");
  await draft.fill("Review integration");
  await draft.press("Enter");
  await expect(draft).toHaveValue("");
  await expect(page.locator("li:visible")).toHaveCount(1);
  await expect(page.getByText("todo:Review integration:active")).toBeVisible();

  await draft.fill("Ship browser proof");
  await draft.press("Enter");
  await expect(page.locator("li:visible")).toHaveCount(2);

  const review = page.locator("li").filter({ hasText: "Review integration" });
  await review.getByRole("button", { name: "Toggle" }).click();
  await expect(review).toContainText("todo:Review integration:done");

  await page.getByRole("button", { name: "Active" }).click();
  await expect(page.getByText("todo:Review integration:done")).toBeHidden();
  await expect(page.getByText("todo:Ship browser proof:active")).toBeVisible();

  await page.getByRole("button", { name: "Completed" }).click();
  const completedReview = page.locator("li").filter({ hasText: "Review integration" });
  await completedReview.getByRole("button", { name: "Edit" }).click();
  await expect(completedReview).toContainText("todo:Review integration edited:done");
  await completedReview.getByRole("button", { name: "Delete" }).click();
  await expect(page.locator("li:visible")).toHaveCount(0);

  await page.getByRole("button", { name: "All" }).click();
  const remaining = page.locator("li").filter({ hasText: "Ship browser proof" });
  await remaining.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("No todos")).toBeVisible();
  await expect(page.locator("li")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
  expect(pageErrors).toEqual([]);
});
