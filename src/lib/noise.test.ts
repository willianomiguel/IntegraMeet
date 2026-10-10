import { describe, expect, it } from "vitest";
import { isNoiseText } from "./noise";

describe("isNoiseText", () => {
  it("reconhece marcadores de som entre colchetes e parênteses", () => {
    expect(isNoiseText("[barulho]")).toBe(true);
    expect(isNoiseText("(música)")).toBe(true);
    expect(isNoiseText("*risos*")).toBe(true);
    expect(isNoiseText("[BLANK_AUDIO]")).toBe(true);
    expect(isNoiseText("[Música]")).toBe(true);
    expect(isNoiseText("♪")).toBe(true);
  });

  it("reconhece texto que é só o nome do som", () => {
    expect(isNoiseText("barulho")).toBe(true);
    expect(isNoiseText("Aplausos")).toBe(true);
  });

  it("não esconde fala de verdade", () => {
    expect(isNoiseText("Vamos começar a reunião.")).toBe(false);
    expect(isNoiseText("[Maria] vou revisar o contrato")).toBe(false);
    expect(isNoiseText("O barulho do servidor atrapalhou o deploy.")).toBe(false);
  });

  it("considera vazio como ruído", () => {
    expect(isNoiseText("   ")).toBe(true);
  });
});
