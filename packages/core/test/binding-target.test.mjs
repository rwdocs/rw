import { test } from "node:test";
import assert from "node:assert/strict";
import { __napiBindingTarget } from "../index.js";

test("the packaged binding reports the native backend", () => {
  assert.equal(__napiBindingTarget, "native");
});
