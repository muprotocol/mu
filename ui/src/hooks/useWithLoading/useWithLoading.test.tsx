import {render, renderHook, screen, waitFor} from "@testing-library/react";
import { describe, expect, test } from "vitest";

import useWithLoading from "@/hooks/useWithLoading/useWithLoading";

describe("useWithLoading", () => {
  test("it should be true by default", () => {
    const promise = new Promise(() => "test");
    const { result } = renderHook(() => useWithLoading(promise));
    const [isLoading] = result.current;

    expect(isLoading).toBeTruthy();
  });

  test("it should be false on promise resolves", () => {
    expect("pending").toBe(false);
  });
});
