export interface User { name?: string }

export function getUserName(user: User | null | undefined): string {
  // BUG: throws TypeError on null/undefined
  return user.name || "Anonymous";
}
