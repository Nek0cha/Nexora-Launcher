import { useQuery } from "@tanstack/react-query";
import { Input, Label, Text, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { systemMemoryMb } from "../api/commands";
import type { JavaMode } from "../bindings/JavaMode";
import type { MemoryMode } from "../bindings/MemoryMode";
import { SLIDER_STEP_MB, sliderMax } from "../features/instances/memorySlider";
import {
  hasMemoryFlag,
  MEMORY_MAX_MB,
  MEMORY_MIN_MB,
  memoryError,
  type SettingsForm,
} from "../features/instances/settingsForm";
import { MemorySlider } from "./MemorySlider";
import { NumberField } from "./NumberField";
import { Select } from "./Select";
import { controlWidth, inputBase, Notice, Row } from "./settingsLayout";

// メモリ、Java、JVM 引数の3つの行。インスタンスの詳細と作成のダイアログで共有する
export function InstanceAdvancedFields({
  form,
  onChange,
}: {
  form: SettingsForm;
  onChange: (patch: Partial<SettingsForm>) => void;
}) {
  const { t } = useTranslation();
  // 物理メモリの取得中や失敗のときは、数値の入力だけを出す
  const { data: totalMb } = useQuery({ queryKey: ["system-memory-mb"], queryFn: systemMemoryMb, staleTime: Infinity });
  const maxMb = totalMb === undefined ? null : sliderMax(totalMb);
  const modes = [
    { id: "auto", label: t("instances.detail.modeAuto") },
    { id: "manual", label: t("instances.detail.modeManual") },
  ] as const;

  return (
    <>
      <Row>
        <Select<MemoryMode>
          label={t("instances.detail.memory")}
          value={form.memoryMode}
          options={[...modes]}
          onChange={(memoryMode) => onChange({ memoryMode })}
          triggerClassName={controlWidth}
        />
        {form.memoryMode === "manual" && maxMb !== null && (
          <MemorySlider
            label={t("instances.detail.memorySlider")}
            value={form.memoryMb}
            min={MEMORY_MIN_MB}
            max={maxMb}
            step={SLIDER_STEP_MB}
            onChange={(memoryMb) => onChange({ memoryMb })}
          />
        )}
        {form.memoryMode === "manual" && (
          <NumberField
            label={t("instances.detail.memoryMb")}
            value={form.memoryMb}
            min={MEMORY_MIN_MB}
            max={MEMORY_MAX_MB}
            inputWidth="w-16"
            onChange={(memoryMb) => onChange({ memoryMb })}
          />
        )}
        {memoryError(form) && <Notice tone="warning">{t("instances.detail.memoryRange")}</Notice>}
      </Row>
      <Row>
        <Select<JavaMode>
          label={t("instances.detail.java")}
          value={form.javaMode}
          options={[...modes]}
          onChange={(javaMode) => onChange({ javaMode })}
          triggerClassName={controlWidth}
        />
        {form.javaMode === "manual" && (
          <TextField value={form.javaPath} onChange={(javaPath) => onChange({ javaPath })} className="flex flex-col gap-1">
            <Label className="text-text">{t("instances.detail.javaPath")}</Label>
            <Input className={`${inputBase} w-full`} spellCheck={false} />
            <Text slot="description" className="text-xs text-text-muted">
              {t("instances.detail.javaPathHint")}
            </Text>
          </TextField>
        )}
      </Row>
      <Row>
        <TextField value={form.jvmArgs} onChange={(jvmArgs) => onChange({ jvmArgs })} className="flex flex-col gap-1">
          <Label className="text-text">{t("instances.detail.jvmArgs")}</Label>
          <Input className={`${inputBase} tabular w-full`} spellCheck={false} />
          <Text slot="description" className="text-xs text-text-muted">
            {t("instances.detail.jvmArgsHelp")}
          </Text>
        </TextField>
        {hasMemoryFlag(form.jvmArgs) && <Notice tone="warning">{t("instances.detail.xmxWarning")}</Notice>}
      </Row>
    </>
  );
}
