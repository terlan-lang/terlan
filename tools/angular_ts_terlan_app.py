"""Generated Angular.ts Todo application fixtures owned by Terlan source."""

from __future__ import annotations

import json


TODO_SOURCE = """module terlan.angular.Todo.

template TodoSummary from "./TodoSummary.terl.html" {
    title: String,
    state: String
}.

pub struct TodoItem {
    id: Int,
    title: String,
    completed: Bool
}.

pub struct TodoState {
    title: String,
    draft: String,
    filter: String,
    next_id: Int
}.

pub app_module(): String ->
    "terlanTodo".

pub controller_name(): String ->
    "TodoController".

pub initial_state(): TodoState ->
    TodoState {
        title: "Terlan Angular Todo",
        draft: "",
        filter: "all",
        next_id: 1
    }.

pub summary(title: String, state: String): Html ->
    TodoSummary(title = title, state = state).

pub can_create(text: String): Bool ->
    text != "".

pub create_item(id: Int, text: String): TodoItem ->
    TodoItem {id: id, title: "todo:" + text, completed: false}.

pub next_id(current: Int): Int ->
    current + 1.

pub clear_draft(): String ->
    "".

pub edit_item(item: TodoItem): TodoItem ->
    TodoItem {
        id: item#TodoItem.id,
        title: item#TodoItem.title + " edited",
        completed: item#TodoItem.completed
    }.

pub toggle_item(item: TodoItem): TodoItem ->
    TodoItem {
        id: item#TodoItem.id,
        title: item#TodoItem.title,
        completed: if {
            item#TodoItem.completed -> false;
            true -> true
        }
    }.

pub keep_item(item: TodoItem, removed_id: Int): Bool ->
    item#TodoItem.id != removed_id.

pub select_filter(filter: String): String ->
    filter.

pub visible(item: TodoItem, filter: String): Bool ->
    if {
        filter == "active" -> if {
            item#TodoItem.completed -> false;
            true -> true
        };
        filter == "completed" -> item#TodoItem.completed;
        true -> true
    }.

pub list_state(count: Int): String ->
    if {
        count == 0 -> "empty";
        true -> "list"
    }.

pub render_row(item: TodoItem): String ->
    if {
        item#TodoItem.completed -> item#TodoItem.title + ":done";
        true -> item#TodoItem.title + ":active"
}.
"""

TODO_TYPED_TEMPLATE = """<section data-state={state}>
  <h1>{title}</h1>
</section>
"""

TODO_APP_MANIFEST = {
    "schema": "terlan.angular-ts.app.v1",
    "source": "src/terlan/angular/Todo.terl",
    "bootstrap": "examples/todo/todo.js",
    "template": "examples/todo/index.html",
    "ownership": "terlan-source",
    "application": {
        "module_export": "app_module",
        "controller_export": "controller_name",
        "initial_state_export": "initial_state",
        "directive": "terlanCallback",
        "di_token": "todoModel",
        "lifecycle_event": "terlan:probe",
    },
    "flows": {
        "create": "create_item",
        "mutate": "toggle_item",
        "delete": "keep_item",
        "filter": "visible",
        "render": "render_row",
    },
}
TODO_APP_MANIFEST_TEXT = json.dumps(TODO_APP_MANIFEST, indent=2, sort_keys=True) + "\n"

TODO_HARNESS_HTML = """<!doctype html>
<html lang="en" ng-app="terlanTodo">
  <head>
    <meta charset="utf-8">
    <title>Terlan Angular Todo</title>
    <style>
      .ng-hide:not(.ng-hide-animate) {
        display: none !important;
      }
    </style>
  </head>
  <body ng-controller="TodoController as todo">
    <main>
      <h1 terlan-callback>{{ todo.title }}</h1>
      <form ng-submit="todo.add(todo.model.draft)">
        <label for="terlan-todo-draft">New todo</label>
        <input id="terlan-todo-draft" name="draft" ng-model="todo.model.draft">
        <button type="submit">Add</button>
      </form>
      <button type="button" ng-click="todo.setFilter('all')">All</button>
      <button type="button" ng-click="todo.setFilter('active')">Active</button>
      <button type="button" ng-click="todo.setFilter('completed')">Completed</button>
      <p ng-if="todo.model.state === 'empty'">No todos</p>
      <ul ng-if="todo.model.state === 'list'">
        <li ng-repeat="item in todo.model.items" ng-show="item.visible">
          <button type="button" ng-click="todo.toggle(item)">Toggle</button>
          <span>{{ item.label }}</span>
          <button type="button" ng-click="todo.edit(item)">Edit</button>
          <button type="button" ng-click="todo.remove(item)">Delete</button>
        </li>
      </ul>
    </main>
    <script type="module" src="./todo.js"></script>
  </body>
</html>
"""

TODO_HARNESS_JS = """import { angular } from "../../../../dist/index.js";
import * as Todo from "../../build/js/modules/terlan/angular/Todo.js";

const app = angular.module(Todo.app_module(), []);

app.directive("terlanCallback", () => ({
  restrict: "A",
  link(_scope, element) {
    element.dataset.terlanDirective = "mounted";
    const onProbe = () => {
      element.dataset.terlanCallback = "invoked";
    };
    element.addEventListener("terlan:probe", onProbe, { once: true });
  },
}));

app.model("todoModel", () => {
  const initial = Todo.initial_state();

  return {
    draft: initial.draft,
    filter: initial.filter,
    nextId: initial.next_id,
    items: [],
    state: Todo.list_state(0),
  };
});

app.controller(Todo.controller_name(), ["todoModel", function TodoController(todoModel) {
  const initial = Todo.initial_state();
  this.title = initial.title;
  this.model = todoModel;

  const refreshItem = (item) => {
    item.visible = Todo.visible(item, todoModel.filter);
    item.label = Todo.render_row(item);
  };

  this.add = (text) => {
    if (!Todo.can_create(text)) return;

    const item = Todo.create_item(todoModel.nextId, text);
    refreshItem(item);
    todoModel.items.push(item);
    todoModel.nextId = Todo.next_id(todoModel.nextId);
    todoModel.draft = Todo.clear_draft();
    todoModel.state = Todo.list_state(todoModel.items.length);
  };
  this.toggle = (item) => {
    Object.assign(item, Todo.toggle_item(item));
    refreshItem(item);
  };
  this.edit = (item) => {
    Object.assign(item, Todo.edit_item(item));
    refreshItem(item);
  };
  this.remove = (item) => {
    const index = todoModel.items.findIndex(
      (candidate) => !Todo.keep_item(candidate, item.id),
    );
    if (index >= 0) todoModel.items.splice(index, 1);
    todoModel.state = Todo.list_state(todoModel.items.length);
  };
  this.setFilter = (filter) => {
    todoModel.filter = Todo.select_filter(filter);
    for (const item of todoModel.items) refreshItem(item);
  };
}]);

angular.bootstrap(document.body, [app.name]);
"""

TODO_BOUNDARY_TEST = """import * as Todo from "../build/js/modules/terlan/angular/Todo.js";

const initial = Todo.initial_state();
const summary = Todo.summary(initial.title, "empty");
const created = Todo.create_item(initial.next_id, "Review");
const toggled = Todo.toggle_item(created);
const edited = Todo.edit_item(created);
const checks = [
  Todo.app_module() === "terlanTodo",
  Todo.controller_name() === "TodoController",
  initial.title === "Terlan Angular Todo",
  summary === `<section data-state="empty">
  <h1>Terlan&#32;Angular&#32;Todo</h1>
</section>
`,
  Todo.can_create("Review") === true && Todo.can_create("") === false,
  created.id === 1 && created.title === "todo:Review" && created.completed === false,
  Todo.next_id(created.id) === 2,
  Todo.clear_draft() === "",
  toggled.completed === true && created.completed === false,
  edited.title === "todo:Review edited",
  Todo.keep_item(created, 2) === true,
  Todo.keep_item(created, 1) === false,
  Todo.visible(created, "active") === true,
  Todo.visible(toggled, "completed") === true,
  Todo.visible(created, "completed") === false,
  Todo.select_filter("active") === "active",
  Todo.list_state(0) === "empty",
  Todo.list_state(2) === "list",
  Todo.render_row(created) === "todo:Review:active",
  Todo.render_row(toggled) === "todo:Review:done",
];

if (!checks.every(Boolean)) {
  throw new Error("Terlan-owned todo app behavior failed");
}
"""

TODO_HARNESS_CONTRACT_TEST = """import fs from "node:fs";

const html = fs.readFileSync("examples/todo/index.html", "utf8");
const js = fs.readFileSync("examples/todo/todo.js", "utf8");
const manifest = JSON.parse(fs.readFileSync("examples/todo/angular-ts.json", "utf8"));

const requiredHtml = [
  'ng-app="terlanTodo"',
  'ng-controller="TodoController as todo"',
  "terlan-callback",
  'ng-submit="todo.add(todo.model.draft)"',
  'ng-model="todo.model.draft"',
  `ng-click="todo.setFilter('active')"`,
  'ng-click="todo.toggle(item)"',
  'ng-click="todo.edit(item)"',
  'ng-click="todo.remove(item)"',
  'ng-repeat="item in todo.model.items"',
  'ng-show="item.visible"',
];
const requiredJs = [
  "angular.module(Todo.app_module(), [])",
  'app.directive("terlanCallback"',
  'element.addEventListener("terlan:probe"',
  'app.model("todoModel"',
  "app.controller(Todo.controller_name()",
  "Todo.initial_state()",
  "Todo.can_create(text)",
  "Todo.create_item(todoModel.nextId, text)",
  "Todo.next_id(todoModel.nextId)",
  "Todo.clear_draft()",
  "Todo.toggle_item(item)",
  "Todo.edit_item(item)",
  "Todo.keep_item(candidate, item.id)",
  "Todo.select_filter(filter)",
  "Todo.visible(item, todoModel.filter)",
  "Todo.render_row(item)",
  "angular.bootstrap(document.body, [app.name])",
];
const forbiddenJs = [
  'angular.module("terlanTodo", [])',
  'item.completed = !item.completed',
  'candidate !== item',
  'this.filter === "active"',
];

for (const marker of requiredHtml) {
  if (!html.includes(marker)) throw new Error(`missing AngularTS todo HTML marker: ${marker}`);
}
for (const marker of requiredJs) {
  if (!js.includes(marker)) throw new Error(`missing Terlan-owned app adapter marker: ${marker}`);
}
for (const marker of forbiddenJs) {
  if (js.includes(marker)) throw new Error(`JavaScript owns todo behavior: ${marker}`);
}
if (manifest.ownership !== "terlan-source") {
  throw new Error("AngularTS app manifest does not assign ownership to Terlan source");
}
if (Object.keys(manifest.flows).sort().join(",") !== "create,delete,filter,mutate,render") {
  throw new Error("AngularTS app manifest does not cover all required user flows");
}
"""

TODO_PLAYWRIGHT_CONFIG = """import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://terlan.test/";

export default defineConfig({
  testDir: ".",
  testMatch: "terlan.test.ts",
  fullyParallel: false,
  workers: 1,
  reporter: "line",
  use: {
    baseURL,
    screenshot: "only-on-failure",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "firefox",
      use: { ...devices["Desktop Firefox"] },
    },
  ],
});
"""

TODO_BROWSER_TEST = """import { expect, test } from "@playwright/test";
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
"""
