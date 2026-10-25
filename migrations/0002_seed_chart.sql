-- Default chart of accounts and funds for a GCC charity. Codes are stable: the
-- application refers to the system accounts below by code.

INSERT INTO accounts (code, name_ar, name_en, account_type, is_postable) VALUES
    ('1000', 'الأصول', 'Assets', 'asset', FALSE),
    ('2000', 'الالتزامات', 'Liabilities', 'liability', FALSE),
    ('3000', 'صافي الأصول', 'Net assets', 'net_assets', FALSE),
    ('4000', 'الإيرادات', 'Revenue', 'revenue', FALSE),
    ('5000', 'مصروفات البرامج', 'Program expenses', 'expense', FALSE),
    ('6000', 'المصروفات الإدارية وجمع التبرعات', 'Support expenses', 'expense', FALSE);

INSERT INTO accounts (code, name_ar, name_en, account_type, parent_id, functional_class)
SELECT v.code, v.name_ar, v.name_en, v.t::account_type, p.id, v.fc::functional_class
FROM (VALUES
    ('1100', 'النقدية بالصندوق', 'Cash on hand', 'asset', '1000', NULL),
    ('1110', 'البنك - الحساب الجاري', 'Bank - current account', 'asset', '1000', NULL),
    ('1120', 'مقاصة بوابات الدفع', 'Payment gateway clearing', 'asset', '1000', NULL),
    ('1200', 'تعهدات تبرع مستحقة', 'Pledges receivable', 'asset', '1000', NULL),
    ('1300', 'مخزون المساعدات العينية', 'In-kind aid inventory', 'asset', '1000', NULL),
    ('1400', 'استثمارات الأوقاف', 'Waqf investments', 'asset', '1000', NULL),
    ('1900', 'مستحق من صناديق أخرى', 'Due from other funds', 'asset', '1000', NULL),
    ('2100', 'الدائنون', 'Accounts payable', 'liability', '2000', NULL),
    ('2200', 'قسائم مساعدات قائمة', 'Aid vouchers outstanding', 'liability', '2000', NULL),
    ('2900', 'مستحق لصناديق أخرى', 'Due to other funds', 'liability', '2000', NULL),
    ('3100', 'صافي أصول غير مقيدة', 'Net assets without donor restrictions', 'net_assets', '3000', NULL),
    ('3200', 'صافي أصول مقيدة مؤقتاً', 'Net assets with donor restrictions', 'net_assets', '3000', NULL),
    ('3300', 'أصل الوقف', 'Endowment (waqf) principal', 'net_assets', '3000', NULL),
    ('4100', 'إيرادات الزكاة', 'Zakat revenue', 'revenue', '4000', NULL),
    ('4200', 'إيرادات الصدقات', 'Sadaqah revenue', 'revenue', '4000', NULL),
    ('4300', 'إيرادات الكفارات', 'Kaffarah revenue', 'revenue', '4000', NULL),
    ('4400', 'تبرعات الأوقاف', 'Waqf contributions', 'revenue', '4000', NULL),
    ('4500', 'تبرعات ومنح عامة', 'General donations and grants', 'revenue', '4000', NULL),
    ('4600', 'عوائد استثمار الأوقاف', 'Waqf investment income', 'revenue', '4000', NULL),
    ('4900', 'فروق العملة والتقريب', 'FX differences and rounding', 'revenue', '4000', NULL),
    ('5100', 'مساعدات نقدية', 'Cash aid', 'expense', '5000', 'program'),
    ('5200', 'مساعدات عينية', 'In-kind aid', 'expense', '5000', 'program'),
    ('5300', 'مساعدات بالقسائم', 'Voucher aid', 'expense', '5000', 'program'),
    ('5400', 'سهم العاملين عليها', 'Zakat administration (amilin)', 'expense', '5000', 'management_general'),
    ('6100', 'الرواتب والأجور', 'Salaries and wages', 'expense', '6000', 'management_general'),
    ('6200', 'تكاليف جمع التبرعات', 'Fundraising costs', 'expense', '6000', 'fundraising'),
    ('6300', 'رسوم بنكية وبوابات الدفع', 'Bank and gateway fees', 'expense', '6000', 'management_general')
) AS v(code, name_ar, name_en, t, parent, fc)
JOIN accounts p ON p.code = v.parent;

INSERT INTO funds (code, name_ar, name_en, restriction, donation_kind, purpose) VALUES
    ('GEN',   'الصندوق العام', 'General fund', 'unrestricted', NULL, 'Operations and unrestricted programs'),
    ('ZAKAT', 'صندوق الزكاة', 'Zakat fund', 'temporarily_restricted', 'zakat', 'Disbursement to the eight asnaf only'),
    ('KAFF',  'صندوق الكفارات', 'Kaffarah fund', 'temporarily_restricted', 'kaffarah', 'Feeding and clothing the poor and needy'),
    ('SADQ',  'صندوق الصدقات المخصصة', 'Designated sadaqah fund', 'temporarily_restricted', 'sadaqah', 'Sadaqah given for a stated purpose'),
    ('WAQF',  'صندوق أصل الوقف', 'Waqf principal fund', 'permanently_restricted', 'waqf', 'Endowment principal; only returns may be spent'),
    ('WAQFR', 'صندوق عوائد الوقف', 'Waqf returns fund', 'temporarily_restricted', NULL, 'Returns spent per the waqif''s conditions');
