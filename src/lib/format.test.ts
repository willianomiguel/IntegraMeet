import { describe, expect, it } from "vitest";
import { formatBytes, formatClock } from "./format";

describe("formatBytes", () => {
  it("usa GB com vírgula em pt-BR", () => {
    expect(formatBytes(1.5 * 1024 ** 3)).toBe("1,5 GB");
  });
  it("usa MB para tamanhos menores", () => {
    expect(formatBytes(488 * 1024 ** 2)).toBe("488 MB");
  });
});

describe("formatClock", () => {
  it("mostra minutos e segundos", () => {
    expect(formatClock(65_000)).toBe("01:05");
  });
  it("mostra horas quando passa de uma hora", () => {
    expect(formatClock(3_725_000)).toBe("1:02:05");
  });
});
