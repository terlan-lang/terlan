import fs from "node:fs";

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
