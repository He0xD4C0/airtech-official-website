import type { CardEntry, ProductFamily } from '@/types/content'

export const productFamilies: ProductFamily[] = [
  {
    id: 'centrifugal',
    name: 'Centrifugal fans',
    slug: 'centrifugal',
    form: 'Centrifugal',
    subtypes: ['Backward-curved', 'Forward-curved'],
    description: 'Browse the catalog structure for centrifugal fan records as verified models are published.',
  },
  {
    id: 'axial',
    name: 'Axial fans',
    slug: 'axial',
    form: 'Axial',
    description: 'A catalog entry point for validated axial fan records and their application relationships.',
  },
  {
    id: 'cross-flow',
    name: 'Cross-flow fans',
    slug: 'cross-flow',
    form: 'Cross-flow',
    description: 'Explore the future home of controlled cross-flow product records and approved resources.',
  },
  {
    id: 'inline-duct',
    name: 'Inline duct fans',
    slug: 'inline-duct',
    form: 'Inline duct',
    description: 'Find validated inline duct products by installation, electrical and operating requirements.',
  },
  {
    id: 'motors',
    name: 'Motors',
    slug: 'motors',
    form: 'Motor platform',
    subtypes: ['AC', 'DC', 'EC'],
    description: 'Discover motor platforms without treating EC technology as a fan-form category.',
  },
]

export const solutions: CardEntry[] = [
  ['hvac', 'HVAC', 'Frame airflow questions around system duty, control and installation constraints.'],
  ['refrigeration', 'Refrigeration', 'Connect thermal-management requirements to verified fan and motor records.'],
  ['data-centers', 'Data centers', 'Structure cooling discovery around duty point, redundancy and control needs.'],
  ['energy-storage', 'Energy storage', 'Capture environmental, electrical and enclosure constraints before selection.'],
  ['air-purification', 'Air purification', 'Relate airflow components to filtration resistance and system integration.'],
  ['cleanroom', 'Cleanroom', 'Document operating, control and compliance requirements for engineering review.'],
  ['industrial-ventilation', 'Industrial ventilation', 'Organize industrial airflow requirements without assuming model suitability.'],
  ['commercial-buildings', 'Commercial buildings', 'Guide product discovery using project-level operating context.'],
].map(([slug, title, summary]) => ({ slug, title, summary, href: `/en/solutions/${slug}` }))

export const technologies: CardEntry[] = [
  ['ec-motor', 'EC motor technology', 'Understand how motor technology is modeled separately from fan form.'],
  ['aerodynamics', 'Aerodynamics', 'Explore the variables behind airflow, pressure and fan-system interaction.'],
  ['airflow-and-pressure', 'Airflow and pressure', 'Keep airflow and pressure units, definitions and conditions explicit.'],
  ['control', 'Control', 'Describe signals, feedback and failure behavior as separate engineering fields.'],
  ['efficiency', 'Efficiency', 'Distinguish motor, fan, system and electrical-input efficiency.'],
  ['noise-and-vibration', 'Noise and vibration', 'Review the measurement context required for meaningful acoustic data.'],
].map(([slug, title, summary]) => ({ slug, title, summary, href: `/en/technology/${slug}` }))

export const articles: CardEntry[] = [
  {
    slug: 'reading-a-fan-curve',
    title: 'Reading a fan curve with the right context',
    summary: 'A practical introduction to airflow, pressure and operating conditions—without substituting for model data.',
    href: '/en/resources/articles/reading-a-fan-curve',
    eyebrow: 'Engineering note',
    tags: ['Airflow', 'Pressure', 'Selection'],
  },
  {
    slug: 'product-data-statuses',
    title: 'Why missing product data should never become zero',
    summary: 'How explicit data states help engineering teams distinguish missing, pending and non-applicable values.',
    href: '/en/resources/articles/product-data-statuses',
    eyebrow: 'Data quality',
    tags: ['Product data', 'Verification'],
  },
]

export const caseEntries: CardEntry[] = [
  {
    slug: 'verified-case-record-preview',
    title: 'Verified case record preview',
    summary: 'A holding template for approved problem, configuration, evidence and outcome records.',
    href: '/en/resources/case-studies/verified-case-record-preview',
    eyebrow: 'Publication held',
    status: 'Awaiting approved evidence',
  },
]

export const faqCategories: CardEntry[] = [
  ['product', 'Product', 'Product identity, revisions and published data.'],
  ['selection', 'Selection', 'Duty points, constraints and engineering review.'],
  ['technical', 'Technical', 'Airflow, pressure, controls and operating conditions.'],
  ['application', 'Application', 'System context and suitability boundaries.'],
  ['customization-oem', 'Customization & OEM', 'Project inputs and engineering discussion.'],
  ['ordering-commercial', 'Ordering & commercial', 'How to provide commercial inquiry context.'],
  ['shipping-delivery', 'Shipping & delivery', 'How delivery details are confirmed for an inquiry.'],
  ['installation-maintenance', 'Installation & maintenance', 'Where approved model documentation is required.'],
  ['replacement-after-sales', 'Replacement & after-sales', 'Existing product context and support routing.'],
].map(([slug, title, summary]) => ({
  slug,
  title,
  summary,
  href: `/en/resources/faqs/${slug}`,
}))

export const resourceSearchEntries: CardEntry[] = [
  ...articles,
  ...technologies,
  ...solutions,
  ...faqCategories,
]
