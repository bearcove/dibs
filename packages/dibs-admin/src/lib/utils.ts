import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
    return twMerge(clsx(inputs));
}

// Types for shadcn-svelte components
export type WithElementRef<T, El = HTMLElement> = T & {
    ref?: El | null;
};

export type WithoutChildren<T> = T extends { children?: any }
    ? Omit<T, "children">
    : T;

export type WithoutChild<T> = T extends { child?: any }
    ? Omit<T, "child">
    : T;

export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
