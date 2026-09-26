pipeline {
  agent none

  options {
    skipDefaultCheckout(true)
    disableConcurrentBuilds(abortPrevious: false)
    buildDiscarder(logRotator(numToKeepStr: '30', artifactNumToKeepStr: '10'))
    timestamps()
    timeout(time: 90, unit: 'MINUTES')
    durabilityHint('MAX_SURVIVABILITY')
  }

  triggers {
    pollSCM('H/3 * * * *')
  }

  environment {
    AIRTEK_IMAGE_PREFIX = 'ghcr.io/he0xd4c0/airtekpower'
    CARGO_HOME = '/var/cache/airtek/cargo'
    PNPM_HOME = '/var/cache/airtek/pnpm'
    PLAYWRIGHT_BROWSERS_PATH = '/var/cache/airtek/playwright'
  }

  stages {
    stage('CI') {
      parallel {
        stage('Frontend and contracts') {
          agent { label 'airtek-builder' }
          steps {
            checkout scm
            sh 'scripts/ci/frontend.sh'
          }
          post {
            always {
              sh 'scripts/ci/cleanup.sh'
              deleteDir()
            }
          }
        }

        stage('Rust and PostgreSQL') {
          agent { label 'airtek-builder' }
          steps {
            checkout scm
            sh 'scripts/ci/rust.sh'
          }
          post {
            always {
              sh 'scripts/ci/cleanup.sh'
              deleteDir()
            }
          }
        }

        stage('Gateway browser contracts') {
          agent { label 'airtek-builder' }
          steps {
            checkout scm
            sh 'scripts/ci/browser.sh'
          }
          post {
            unsuccessful {
              archiveArtifacts artifacts: '.local/qa/playwright-report/**/*,.local/qa/test-results/**/*', allowEmptyArchive: true
            }
            always {
              sh 'scripts/ci/cleanup.sh'
              deleteDir()
            }
          }
        }
      }
    }

    stage('Publish immutable images') {
      when { branch 'cicd' }
      agent { label 'airtek-builder' }
      options { lock(resource: 'airtek-ghcr-publish') }
      steps {
        checkout scm
        withCredentials([usernamePassword(
          credentialsId: 'airtek-ghcr-push',
          usernameVariable: 'GHCR_USERNAME',
          passwordVariable: 'GHCR_TOKEN'
        )]) {
          sh 'printf \'%s\' "$GHCR_TOKEN" | docker login ghcr.io --username "$GHCR_USERNAME" --password-stdin'
          sh 'scripts/ci/publish-images.sh'
          sh 'docker logout ghcr.io'
        }
      }
      post {
        always {
          archiveArtifacts artifacts: 'reports/**/*', allowEmptyArchive: true, fingerprint: true
          sh 'docker logout ghcr.io >/dev/null 2>&1 || true'
        }
      }
    }

    stage('Deploy') {
      when {
        allOf {
          branch 'cicd'
          expression { env.CD_ENABLED == 'true' }
        }
      }
      agent { label 'airtek-builder' }
      options { lock(resource: 'airtek-production-deploy') }
      steps {
        checkout scm
        withCredentials([file(credentialsId: 'airtek-prod-known-hosts', variable: 'AIRTEK_KNOWN_HOSTS')]) {
          sshagent(credentials: ['airtek-prod-ssh']) {
            sh 'scripts/ci/deploy-release.sh'
          }
        }
      }
    }

    stage('Published, deployment disabled') {
      when {
        allOf {
          branch 'cicd'
          expression { env.CD_ENABLED != 'true' }
        }
      }
      agent { label 'airtek-builder' }
      steps {
        echo 'PUBLISHED_NOT_DEPLOYED: CD_ENABLED is false; no SSH connection was attempted.'
      }
    }
  }

  post {
    always {
      script {
        if (env.BRANCH_NAME == 'main') {
          echo 'CI_ONLY: main never logs in to GHCR and never deploys.'
        }
      }
    }
  }
}
