export type Locale = "zh-CN" | "en";

export type MessageValue = string | Messages;

export interface Messages {
  [key: string]: MessageValue;
}
