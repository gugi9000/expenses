-- Last used payout account, pre-filled on the next expense sheet.
ALTER TABLE users ADD COLUMN bank_account TEXT;
-- Snapshot on the sheet, so later changes to the user's account don't alter old sheets.
ALTER TABLE expense_sheets ADD COLUMN bank_account TEXT;
